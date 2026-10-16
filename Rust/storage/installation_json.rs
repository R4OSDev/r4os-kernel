//! Bounded generic JSON decoding for the installation file. Nodes and escaped
//! strings use the caller's original scratch arena. Container traversal is
//! iterative, so nesting does not consume the shared kernel task stack.
use core::{marker::PhantomData, mem, ptr, slice};
type Result<T> = core::result::Result<T, &'static [u8]>;
const SYNTAX: &[u8] = b"SyntaxError";
const END: &[u8] = b"UnexpectedEndOfInput";
#[derive(Clone, Copy)]
struct Span {
    data: *const u8,
    len: usize,
}
impl Span {
    const EMPTY: Self = Self {
        data: ptr::null(),
        len: 0,
    };
    unsafe fn bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.data, self.len) }
        }
    }
}
#[derive(Clone, Copy)]
struct Token {
    parent: *mut Token,
    first: *mut Token,
    last: *mut Token,
    next: *mut Token,
    key: Span,
    text: Span,
    len: usize,
    kind: u8,
    state: u8,
}
impl Token {
    const EMPTY: Self = Self {
        parent: ptr::null_mut(),
        first: ptr::null_mut(),
        last: ptr::null_mut(),
        next: ptr::null_mut(),
        key: Span::EMPTY,
        text: Span::EMPTY,
        len: 0,
        kind: 0,
        state: 0,
    };
}
#[derive(Clone, Copy)]
pub(super) struct Node<'a> {
    token: *const Token,
    lifetime: PhantomData<&'a [u8]>,
}
pub(super) struct Meta<'a> {
    pub kind: u8,
    pub len: usize,
    pub text: &'a [u8],
}
impl<'a> Node<'a> {
    pub(super) fn meta(self) -> Meta<'a> {
        let t = unsafe { self.token.read() };
        Meta {
            kind: t.kind,
            len: t.len,
            text: unsafe { t.text.bytes() },
        }
    }
    pub(super) fn field(self, name: &[u8]) -> Option<Self> {
        let mut t = unsafe { (*self.token).first };
        while !t.is_null() {
            if unsafe { (*t).key.bytes() } == name {
                return Some(Self {
                    token: t,
                    lifetime: PhantomData,
                });
            }
            t = unsafe { (*t).next };
        }
        None
    }
    pub(super) fn at(self, index: usize) -> Option<Self> {
        let mut t = unsafe { (*self.token).first };
        let mut i = 0;
        while !t.is_null() {
            if i == index {
                return Some(Self {
                    token: t,
                    lifetime: PhantomData,
                });
            }
            t = unsafe { (*t).next };
            i += 1;
        }
        None
    }
}
struct Decoder<'a> {
    input: &'a [u8],
    cursor: usize,
    arena: *mut u8,
    capacity: usize,
    used: usize,
}
impl Decoder<'_> {
    fn allocate(&mut self, len: usize, align: usize) -> Result<*mut u8> {
        let address = self
            .arena
            .addr()
            .checked_add(self.used)
            .ok_or(b"OutOfMemory".as_slice())?;
        let padding = address.wrapping_neg() & (align - 1);
        let start = self
            .used
            .checked_add(padding)
            .ok_or(b"OutOfMemory".as_slice())?;
        let end = start.checked_add(len).ok_or(b"OutOfMemory".as_slice())?;
        if end > self.capacity {
            return Err(b"OutOfMemory");
        }
        self.used = end;
        Ok(unsafe { self.arena.add(start) })
    }
    fn node(&mut self, parent: *mut Token, key: Span) -> Result<*mut Token> {
        let t = self
            .allocate(mem::size_of::<Token>(), mem::align_of::<Token>())?
            .cast::<Token>();
        unsafe {
            t.write(Token {
                parent,
                key,
                ..Token::EMPTY
            });
            if !parent.is_null() {
                if (*parent).first.is_null() {
                    (*parent).first = t;
                } else {
                    (*(*parent).last).next = t;
                }
                (*parent).last = t;
                (*parent).len += 1;
            }
        }
        Ok(t)
    }
    fn whitespace(&mut self) {
        while self
            .input
            .get(self.cursor)
            .is_some_and(|b| b" \t\r\n".contains(b))
        {
            self.cursor += 1;
        }
    }
    fn peek(&mut self) -> Result<u8> {
        self.whitespace();
        self.input.get(self.cursor).copied().ok_or(END)
    }
    fn expect(&mut self, byte: u8) -> Result<()> {
        if self.peek()? != byte {
            return Err(SYNTAX);
        }
        self.cursor += 1;
        Ok(())
    }
    fn hex4(&self, at: usize, end: usize) -> Result<u32> {
        if at + 4 > end {
            return Err(SYNTAX);
        }
        let mut n = 0;
        for &b in &self.input[at..at + 4] {
            n = (n << 4)
                | u32::from(match b {
                    b'0'..=b'9' => b - b'0',
                    b'a'..=b'f' => b - b'a' + 10,
                    b'A'..=b'F' => b - b'A' + 10,
                    _ => return Err(SYNTAX),
                });
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<Span> {
        self.expect(b'"')?;
        let start = self.cursor;
        let mut end = start;
        let mut escaped = false;
        loop {
            let b = *self.input.get(end).ok_or(END)?;
            if b == b'"' {
                break;
            }
            if b < 0x20 {
                return Err(SYNTAX);
            }
            if b == b'\\' {
                escaped = true;
                end += 1;
                self.input.get(end).ok_or(END)?;
            }
            end += 1;
        }
        self.cursor = end + 1;
        if !escaped {
            return Ok(Span {
                data: unsafe { self.input.as_ptr().add(start) },
                len: end - start,
            });
        }
        let out = self.allocate(end - start, 1)?;
        let mut written = 0;
        let mut at = start;
        while at < end {
            let b = self.input[at];
            at += 1;
            if b != b'\\' {
                unsafe {
                    out.add(written).write(b);
                }
                written += 1;
                continue;
            }
            let code = *self.input.get(at).ok_or(END)?;
            at += 1;
            let byte = match code {
                b'"' => b'"',
                b'\\' => b'\\',
                b'/' => b'/',
                b'b' => 8,
                b'f' => 12,
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'u' => {
                    let mut n = self.hex4(at, end)?;
                    at += 4;
                    if (0xd800..=0xdbff).contains(&n) {
                        if at + 6 > end || &self.input[at..at + 2] != b"\\u" {
                            return Err(SYNTAX);
                        }
                        let low = self.hex4(at + 2, end)?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(SYNTAX);
                        }
                        n = 0x10000 + ((n - 0xd800) << 10) + low - 0xdc00;
                        at += 6;
                    } else if (0xdc00..=0xdfff).contains(&n) {
                        return Err(SYNTAX);
                    }
                    let ch = char::from_u32(n).ok_or(SYNTAX)?;
                    let mut bytes = [0; 4];
                    let bytes = ch.encode_utf8(&mut bytes).as_bytes();
                    unsafe {
                        ptr::copy_nonoverlapping(bytes.as_ptr(), out.add(written), bytes.len());
                    }
                    written += bytes.len();
                    continue;
                }
                _ => return Err(SYNTAX),
            };
            unsafe {
                out.add(written).write(byte);
            }
            written += 1;
        }
        Ok(Span {
            data: out,
            len: written,
        })
    }
    fn digits(&mut self) -> Result<()> {
        let start = self.cursor;
        while self.input.get(self.cursor).is_some_and(u8::is_ascii_digit) {
            self.cursor += 1;
        }
        if start == self.cursor {
            return Err(if self.cursor == self.input.len() {
                END
            } else {
                SYNTAX
            });
        }
        Ok(())
    }
    fn number(&mut self) -> Result<Span> {
        let start = self.cursor;
        if self.input[self.cursor] == b'-' {
            self.cursor += 1;
        }
        match self.input.get(self.cursor) {
            Some(b'0') => {
                self.cursor += 1;
                if self.input.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                    return Err(SYNTAX);
                }
            }
            Some(b'1'..=b'9') => self.digits()?,
            Some(_) => return Err(SYNTAX),
            None => return Err(END),
        }
        if self.input.get(self.cursor) == Some(&b'.') {
            self.cursor += 1;
            self.digits()?;
        }
        if self
            .input
            .get(self.cursor)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.cursor += 1;
            if self
                .input
                .get(self.cursor)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.cursor += 1;
            }
            self.digits()?;
        }
        Ok(Span {
            data: unsafe { self.input.as_ptr().add(start) },
            len: self.cursor - start,
        })
    }
    fn complete(&self, node: *mut Token) -> Result<()> {
        let t = unsafe { node.read() };
        if !t.parent.is_null() && unsafe { (*t.parent).kind == 1 } {
            let mut previous = unsafe { (*t.parent).first };
            while previous != node {
                if unsafe { (*previous).key.bytes() == t.key.bytes() } {
                    return Err(b"DuplicateField");
                }
                previous = unsafe { (*previous).next };
            }
        }
        Ok(())
    }
    fn value(&mut self, t: *mut Token) -> Result<()> {
        match self.peek()? {
            b'{' => {
                self.cursor += 1;
                unsafe {
                    (*t).kind = 1;
                }
            }
            b'[' => {
                self.cursor += 1;
                unsafe {
                    (*t).kind = 2;
                }
            }
            b'"' => {
                let s = self.string()?;
                unsafe {
                    (*t).kind = 3;
                    (*t).text = s;
                    (*t).len = s.len;
                }
            }
            b'-' | b'0'..=b'9' => {
                let n = self.number()?;
                unsafe {
                    (*t).kind = 4;
                    (*t).text = n;
                    (*t).len = n.len;
                }
            }
            b't' | b'f' | b'n' => {
                let literal: &[u8] = match self.input[self.cursor] {
                    b't' => b"true",
                    b'f' => b"false",
                    _ => b"null",
                };
                for &b in literal {
                    if self.input.get(self.cursor).copied().ok_or(END)? != b {
                        return Err(SYNTAX);
                    }
                    self.cursor += 1;
                }
            }
            _ => return Err(SYNTAX),
        }
        Ok(())
    }
    fn tree(&mut self) -> Result<*const Token> {
        let root = self.node(ptr::null_mut(), Span::EMPTY)?;
        self.value(root)?;
        let mut current = if unsafe { matches!((*root).kind, 1 | 2) } {
            root
        } else {
            ptr::null_mut()
        };
        while !current.is_null() {
            let t = unsafe { current.read() };
            let next = self.peek()?;
            if t.kind == 1 {
                match t.state {
                    0 | 4 => {
                        if next == b'}' && t.state == 0 {
                            self.cursor += 1;
                            self.complete(current)?;
                            current = t.parent;
                            continue;
                        }
                        let key = self.string()?;
                        self.node(current, key)?;
                        unsafe {
                            (*current).state = 1;
                        }
                    }
                    1 => {
                        self.expect(b':')?;
                        unsafe {
                            (*current).state = 2;
                        }
                    }
                    2 => {
                        self.value(t.last)?;
                        unsafe {
                            (*current).state = 3;
                            if matches!((*t.last).kind, 1 | 2) {
                                current = t.last;
                            } else {
                                self.complete(t.last)?;
                            }
                        }
                    }
                    3 => match next {
                        b',' => {
                            self.cursor += 1;
                            unsafe {
                                (*current).state = 4;
                            }
                        }
                        b'}' => {
                            self.cursor += 1;
                            self.complete(current)?;
                            current = t.parent;
                        }
                        _ => return Err(SYNTAX),
                    },
                    _ => unreachable!(),
                }
            } else {
                match t.state {
                    0 | 4 => {
                        if next == b']' && t.state == 0 {
                            self.cursor += 1;
                            self.complete(current)?;
                            current = t.parent;
                            continue;
                        }
                        let child = self.node(current, Span::EMPTY)?;
                        self.value(child)?;
                        unsafe {
                            (*current).state = 3;
                            if matches!((*child).kind, 1 | 2) {
                                current = child;
                            }
                        }
                    }
                    3 => match next {
                        b',' => {
                            self.cursor += 1;
                            unsafe {
                                (*current).state = 4;
                            }
                        }
                        b']' => {
                            self.cursor += 1;
                            self.complete(current)?;
                            current = t.parent;
                        }
                        _ => return Err(SYNTAX),
                    },
                    _ => unreachable!(),
                }
            }
        }
        self.whitespace();
        if self.cursor != self.input.len() {
            return Err(SYNTAX);
        }
        Ok(root)
    }
}
pub(super) fn decode<'a>(input: &'a [u8], scratch: &'a mut [u8]) -> Result<Node<'a>> {
    if core::str::from_utf8(input).is_err() {
        return Err(SYNTAX);
    }
    let mut decoder = Decoder {
        input,
        cursor: 0,
        arena: scratch.as_mut_ptr(),
        capacity: scratch.len(),
        used: 0,
    };
    let root = decoder.tree()?;
    Ok(Node {
        token: root,
        lifetime: PhantomData,
    })
}
