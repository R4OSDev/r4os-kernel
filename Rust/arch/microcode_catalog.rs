//! Exact vendor file origins admitted by this kernel artifact. Distribution
//! ships the original files; changing this allowlist needs a new kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pin {
    pub vendor: u32,
    pub id: u32,
    pub signature: u32,
    pub revision: u32,
    pub minimum_revision: u32,
    pub platform_mask: u32,
    pub bytes: usize,
    pub sha256: [u8; 32],
}
pub(crate) const PINS: [Pin; 2] = [
    Pin {
        vendor: 1,
        id: 1,
        signature: 0x306c3,
        revision: 0x28,
        minimum_revision: 0,
        platform_mask: 0x32,
        bytes: 23552,
        sha256: [
            0x2a, 0xf4, 0x1b, 0xcb, 0xe3, 0x41, 0x2a, 0xec, 0x07, 0xc7, 0x20, 0xe2, 0xbe, 0x6e,
            0xbd, 0xd0, 0x92, 0xad, 0xaf, 0x8a, 0x76, 0x35, 0xef, 0xef, 0x6c, 0x54, 0x80, 0xa0,
            0xb5, 0x3a, 0x18, 0x5d,
        ],
    },
    // AMD's supplied README requires base0x0a0011d9 for this signed patch.
    // The older0x0a0011d5 fallback in the same file is never selected to bypass it.
    Pin {
        vendor: 2,
        id: 2,
        signature: 0x00a00f11,
        revision: 0x0a0011de,
        minimum_revision: 0x0a0011d9,
        platform_mask: 0,
        bytes: 128644,
        sha256: [
            0xc6, 0x14, 0xd6, 0xdb, 0x80, 0x56, 0xc5, 0xc6, 0x7a, 0x91, 0x89, 0xb2, 0x25, 0x12,
            0x41, 0x27, 0xd5, 0x69, 0x90, 0xa1, 0x90, 0x30, 0x5b, 0xcb, 0x39, 0x27, 0xd5, 0x0e,
            0x13, 0x2d, 0xe7, 0xdd,
        ],
    },
];
