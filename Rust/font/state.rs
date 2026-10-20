//! Immutable published font-state queries and exclusive unpublished construction.
//! Large glyph arrays are copied directly between resident fields, never by value.
use super::{builtin, decode, types::*};
use core::ptr;
pub fn cached_row_mask(c: &CachedFont, id: usize, row: u32) -> u64 {
    if id >= MAX_GLYPHS || row >= u32::from(c.height) {
        return 0;
    }
    let width = u32::from(c.widths[id]).min(u32::from(MAX_GLYPH_W));
    let mut result = 0;
    for column in 0..width {
        let byte = (column / 8) as usize;
        if byte >= usize::from(c.bytes_per_row) || byte >= MAX_BYTES_PER_ROW {
            break;
        }
        let shift = 7 - (column & 7);
        if (c.glyphs[id][row as usize][byte] >> shift) & 1 == 1 {
            result |= 1u64 << column;
        }
    }
    result
}
impl CatalogState {
    pub fn reset(&mut self) {
        self.active_present = false;
        self.active_font_id = 0;
    }
    pub fn reset_catalog(&mut self) {
        self.catalog.fill(CatalogEntry::EMPTY);
        self.catalog_entries = 0;
        self.active_present = false;
        self.active_font_id = 0;
    }
    pub fn glyph_width(&self) -> u32 {
        if self.active_present {
            u32::from(self.active.max_advance)
        } else {
            8
        }
    }
    pub fn glyph_height(&self) -> u32 {
        if self.active_present {
            u32::from(self.active.line_height)
        } else {
            8
        }
    }
    pub fn baseline(&self) -> i16 {
        if self.active_present {
            self.active.baseline
        } else {
            7
        }
    }
    pub fn catalog_count(&self) -> usize {
        self.catalog_entries
    }
    pub fn catalog_entry(&self, index: usize) -> Option<CatalogEntry> {
        if index >= self.catalog_entries {
            None
        } else {
            Some(self.catalog[index])
        }
    }
    pub fn font_count(&self) -> usize {
        1 + self.catalog_entries
    }
    pub fn font_id_for_catalog_index(&self, index: usize) -> u32 {
        if index >= self.catalog_entries {
            0
        } else {
            u32::try_from(index + 1).expect("Original bounded font ID")
        }
    }
    fn catalog_index(&self, id: u32) -> Option<usize> {
        if id == 0 {
            return None;
        }
        let index = (id - 1) as usize;
        if index >= self.catalog_entries {
            None
        } else {
            Some(index)
        }
    }
    pub fn is_renderable(&self, id: u32) -> bool {
        if id == 0 {
            return true;
        }
        let Some(index) = self.catalog_index(id) else {
            return false;
        };
        self.catalog[index].renderable
            && unsafe { ptr::addr_of!((*self.cache_ptr(index)).used).read() }
    }
    pub fn normalize_id(&self, id: u32) -> u32 {
        if self.is_renderable(id) { id } else { 0 }
    }
    pub fn current_font_id(&self) -> u32 {
        self.normalize_id(self.active_font_id)
    }
    pub fn catalog_entry_for_id(&self, id: u32) -> Option<CatalogEntry> {
        self.catalog_index(id).map(|i| self.catalog[i])
    }
    fn cache_ptr(&self, index: usize) -> *mut CachedFont {
        assert!(
            index < self.cached_fonts_len,
            "Original cached-font slice bounds"
        );
        assert!(!self.cached_fonts.is_null());
        unsafe { self.cached_fonts.add(index) }
    }
    fn cached_font(&self, id: u32) -> Option<&CachedFont> {
        let index = self.catalog_index(id)?;
        let p = self.cache_ptr(index);
        if !unsafe { ptr::addr_of!((*p).used).read() } {
            return None;
        } // Only used=true publishes a completely initialized slot.
        Some(unsafe { &*p })
    }
    fn active_glyph_id(&self, cp: u32) -> Option<usize> {
        decode::indexed_id(&self.active_index, cp, None)
    }
    pub fn glyph_row(&self, c: u8, row: u32) -> u8 {
        self.glyph_row_scalar(u32::from(c), row)
    }
    pub fn glyph_row_scalar(&self, c: u32, row: u32) -> u8 {
        if self.active_present {
            if row >= u32::from(self.active.height) {
                return 0;
            }
            if let Some(id) = self.active_glyph_id(c) {
                return self.active_glyphs[id][row as usize][0];
            }
        }
        if row >= 8 {
            0
        } else {
            builtin::glyph(c)[row as usize]
        }
    }
    pub fn glyph_pixel(&self, c: u8, row: u32, col: u32) -> bool {
        if self.active_present {
            let f = &self.active;
            if row >= u32::from(f.height) || col >= u32::from(MAX_GLYPH_W) {
                return false;
            }
            if let Some(id) = self.active_glyph_id(u32::from(c)) {
                if col >= u32::from(self.active_glyph_widths[id]) {
                    return false;
                }
                let byte = (col / 8) as usize;
                if byte >= usize::from(f.bytes_per_row) || byte >= MAX_BYTES_PER_ROW {
                    return false;
                }
                return (self.active_glyphs[id][row as usize][byte] >> (7 - (col & 7))) & 1 == 1;
            }
        }
        if row >= 8 || col >= 8 {
            false
        } else {
            (builtin::glyph(u32::from(c))[row as usize] >> (7 - col)) & 1 == 1
        }
    }
    pub fn glyph_bitmap_width(&self, c: u8) -> u32 {
        if self.active_present
            && let Some(id) = self.active_glyph_id(u32::from(c))
        {
            return u32::from(self.active_glyph_widths[id]);
        }
        8
    }
    pub fn glyph_advance(&self, c: u8) -> u32 {
        if self.active_present
            && let Some(id) = self.active_glyph_id(u32::from(c))
        {
            return u32::from(self.active_glyph_advances[id]);
        }
        8
    }
    pub fn glyph_height_for_font(&self, id: u32) -> u32 {
        self.cached_font(id).map_or(8, |f| u32::from(f.line_height))
    }
    pub fn glyph_advance_for_font(&self, id: u32, cp: u32) -> u32 {
        if let Some(f) = self.cached_font(id)
            && let Some(glyph) = decode::indexed_id(&f.index, cp, None)
        {
            return u32::from(f.advances[glyph]);
        }
        8
    }
    pub fn glyph_bitmap_width_for_font(&self, id: u32, cp: u32) -> u32 {
        if let Some(f) = self.cached_font(id)
            && let Some(glyph) = decode::indexed_id(&f.index, cp, None)
        {
            return u32::from(f.widths[glyph]);
        }
        8
    }
    pub fn glyph_pixel_for_font(&self, id: u32, cp: u32, row: u32, col: u32) -> bool {
        if let Some(f) = self.cached_font(id) {
            if row >= u32::from(f.height) || col >= u32::from(MAX_GLYPH_W) {
                return false;
            }
            if let Some(glyph) = decode::indexed_id(&f.index, cp, None) {
                if col >= u32::from(f.widths[glyph]) {
                    return false;
                }
                let byte = (col / 8) as usize;
                if byte >= usize::from(f.bytes_per_row) || byte >= MAX_BYTES_PER_ROW {
                    return false;
                }
                return (f.glyphs[glyph][row as usize][byte] >> (7 - (col & 7))) & 1 == 1;
            }
        }
        if row >= 8 || col >= 8 {
            false
        } else {
            (builtin::glyph(cp)[row as usize] >> (7 - col)) & 1 == 1
        }
    }
    pub fn glyph_row_mask_for_font(&self, id: u32, cp: u32, row: u32) -> u64 {
        if let Some(f) = self.cached_font(id) {
            if row >= u32::from(f.height) {
                return 0;
            }
            if let Some(glyph) = decode::indexed_id(&f.index, cp, None) {
                return cached_row_mask(f, glyph, row);
            }
        }
        builtin::row_mask(cp, row)
    }
    pub fn glyph_bitmap_for_font(&self, id: u32, cp: u32) -> GlyphBitmap {
        if let Some(f) = self.cached_font(id) {
            let mut out = GlyphBitmap {
                height: u32::from(f.height),
                line_height: u32::from(f.line_height),
                baseline: f.baseline,
                ..GlyphBitmap::EMPTY
            };
            if let Some(glyph) = decode::indexed_id(&f.index, cp, None) {
                out.width = u32::from(f.widths[glyph]);
                out.advance = u32::from(f.advances[glyph]);
                for row in 0..u32::from(f.height) {
                    out.rows[row as usize] = cached_row_mask(f, glyph, row)
                }
                return out;
            }
            out.fallback = true;
            for row in 0..u32::from(f.height).min(8) {
                out.rows[row as usize] = builtin::row_mask(cp, row)
            }
            return out;
        }
        let mut out = GlyphBitmap::EMPTY;
        for row in 0..8 {
            out.rows[row as usize] = builtin::row_mask(cp, row)
        }
        out
    }
    pub fn baseline_for_font(&self, id: u32) -> i16 {
        self.cached_font(id).map_or(7, |f| f.baseline)
    }
    pub fn max_advance_for_font(&self, id: u32) -> u32 {
        self.cached_font(id).map_or(8, |f| u32::from(f.max_advance))
    }
    /// Measure a bounded zero-terminated raw span under the caller's exact lease.
    /// # Safety
    /// The readable text bound and catalog/task pins survive the full operation.
    pub unsafe fn measure_z_raw(&self, id: u32, text: *const u8, max: usize) -> TextMetrics {
        let mut len = 0;
        while len < max && unsafe { text.add(len).read() } != 0 {
            len += 1;
        }
        unsafe { self.measure_raw(id, text, len) }
    }
    /// Raw caller bytes remain raw across any fault; the exact catalogue lease
    /// and task unwind pin outlive the complete metadata/glyph operation.
    /// # Safety
    /// Caller retains the readable text span, exact resident catalog read lease and task unwind pin through the complete measurement.
    pub unsafe fn measure_raw(&self, id: u32, text: *const u8, len: usize) -> TextMetrics {
        let mut line_w = 0u32;
        let mut max_w = 0u32;
        let mut lines = 1u32;
        let mut visible = 0usize;
        let id = self.normalize_id(id);
        let mut cursor = 0;
        while cursor < len {
            let decoded = unsafe { decode::decode_raw(text, len, cursor) };
            cursor += decoded.consumed;
            if decoded.codepoint == u32::from(b'\r') {
                continue;
            }
            if decoded.codepoint == u32::from(b'\n') {
                max_w = max_w.max(line_w);
                line_w = 0;
                lines += 1;
                visible += decoded.consumed;
                continue;
            }
            line_w += self.glyph_advance_for_font(id, decoded.codepoint);
            visible += decoded.consumed;
        }
        max_w = max_w.max(line_w);
        let lh = self.glyph_height_for_font(id);
        TextMetrics {
            width: max_w,
            height: lines * lh,
            baseline: self.baseline_for_font(id),
            line_height: lh,
            visible_bytes: visible,
            ..TextMetrics::default()
        }
    }
    pub fn load_named(&mut self, path: &[u8], bytes: &[u8]) -> bool {
        let Some(parsed) = decode::parse(bytes) else {
            return false;
        };
        if !parsed.renderable {
            return false;
        }
        let temp = ptr::addr_of_mut!(self.temp_cached_font);
        if !unsafe { decode::fill_cached(parsed, temp) } {
            return false;
        } // No CachedFont reference aliases self while copying its resident fields.
        unsafe {
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*temp).glyphs),
                ptr::addr_of_mut!(self.active_glyphs),
                1,
            );
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*temp).widths),
                ptr::addr_of_mut!(self.active_glyph_widths),
                1,
            );
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*temp).advances),
                ptr::addr_of_mut!(self.active_glyph_advances),
                1,
            );
            ptr::copy_nonoverlapping(
                ptr::addr_of!((*temp).index),
                ptr::addr_of_mut!(self.active_index),
                1,
            );
        }
        self.active = ActiveFont {
            width: parsed.width,
            height: parsed.height,
            bytes_per_row: parsed.bytes_per_row,
            max_advance: parsed.max_advance,
            ascent: parsed.ascent,
            descent: parsed.descent,
            line_height: parsed.line_height,
            baseline: parsed.baseline,
            ..ActiveFont::EMPTY
        };
        self.active.family_len = copy_field(&mut self.active.family, parsed.family);
        self.active.face_len = copy_field(&mut self.active.face, parsed.face);
        self.active.path_len = copy_field(&mut self.active.path, path);
        self.active_present = true;
        for entry in &mut self.catalog[..self.catalog_entries] {
            entry.selected =
                !path.is_empty() && decode::path_equals(&entry.path[..entry.path_len], path);
        }
        self.active_font_id = if path.is_empty() {
            0
        } else {
            self.catalog[..self.catalog_entries]
                .iter()
                .position(|e| decode::path_equals(&e.path[..e.path_len], path))
                .map_or(0, |i| self.font_id_for_catalog_index(i))
        };
        true
    }
    pub fn register(&mut self, path: &[u8], bytes: &[u8]) -> bool {
        let parsed = decode::parse(bytes);
        let mut entry = CatalogEntry::EMPTY;
        entry.path_len = copy_field(&mut entry.path, path);
        if let Some(p) = parsed {
            entry.used = true;
            entry.kind = p.kind;
            entry.weight = p.weight;
            entry.style_flags = p.style_flags;
            entry.charset_flags = p.charset_flags;
            entry.face_count = p.face_count;
            entry.strike_count = p.strike_count;
            entry.glyph_count = p.glyph_count;
            entry.renderable = p.renderable;
            entry.width = p.width;
            entry.height = p.height;
            entry.max_advance = p.max_advance;
            entry.ascent = p.ascent;
            entry.descent = p.descent;
            entry.line_height = p.line_height;
            entry.baseline = p.baseline;
            entry.family_len = copy_field(&mut entry.family, p.family);
            entry.face_len = copy_field(&mut entry.face, p.face);
            entry.style_len = copy_field(&mut entry.style, p.style);
            entry.status_len = copy_field(&mut entry.status, p.reason);
        } else {
            entry.used = true;
            entry.kind = FontKind::Unknown;
            entry.renderable = false;
            entry.family_len = copy_field(&mut entry.family, b"Invalid");
            entry.face_len = copy_field(&mut entry.face, b"Invalid R4F");
            entry.style_len = copy_field(&mut entry.style, b"Error");
            entry.status_len = copy_field(&mut entry.status, b"invalid R4F1");
        }
        if self.catalog_entries >= MAX_CATALOG_FONTS {
            return false;
        }
        let slot = self.catalog_entries;
        self.catalog[slot] = entry;
        self.catalog_entries += 1;
        let cache = self.cache_ptr(slot);
        unsafe { ptr::addr_of_mut!((*cache).used).write(false) };
        if let Some(p) = parsed
            && p.renderable
            && !unsafe { decode::fill_cached(p, cache) }
        {
            unsafe { reset_cached(cache) }
            self.catalog[slot].renderable = false;
            self.catalog[slot].status_len =
                copy_field(&mut self.catalog[slot].status, b"cache failed");
        }
        self.catalog[slot].renderable
    }
}
