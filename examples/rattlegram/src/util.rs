pub fn get_be_bit(buf: &[u8], pos: usize) -> bool {
    (buf[pos / 8] >> (7 - pos % 8)) & 1 == 1
}

pub fn set_be_bit(buf: &mut [u8], pos: usize, val: bool) {
    let val = val as u8;
    buf[pos / 8] = (!(1 << (7 - pos % 8)) & buf[pos / 8]) | (val << (7 - pos % 8));
}

pub fn xor_be_bit(buf: &mut [u8], pos: usize, val: bool) {
    buf[pos / 8] ^= (val as u8) << (7 - pos % 8);
}

pub fn get_le_bit(buf: &[u8], pos: usize) -> bool {
    (buf[pos / 8] >> (pos % 8)) & 1 == 1
}

pub const BASE37_BITMAP: [u8; 407] = [
    0, 60, 8, 60, 60, 2, 126, 28, 126, 60, 60, 60, 124, 60, 120, 126, 126, 60, 66, 56, 14, 66, 64,
    130, 66, 60, 124, 60, 124, 60, 254, 66, 66, 130, 66, 130, 126, 0, 66, 24, 66, 66, 6, 64, 32, 2,
    66, 66, 66, 66, 66, 68, 64, 64, 66, 66, 16, 4, 68, 64, 198, 66, 66, 66, 66, 66, 66, 16, 66, 66,
    130, 66, 130, 2, 0, 66, 40, 66, 66, 10, 64, 64, 2, 66, 66, 66, 66, 66, 66, 64, 64, 66, 66, 16,
    4, 72, 64, 170, 66, 66, 66, 66, 66, 64, 16, 66, 66, 130, 36, 68, 2, 0, 70, 8, 2, 2, 18, 64, 64,
    4, 66, 66, 66, 66, 64, 66, 64, 64, 64, 66, 16, 4, 80, 64, 146, 98, 66, 66, 66, 66, 64, 16, 66,
    66, 130, 36, 68, 4, 0, 74, 8, 4, 28, 34, 124, 124, 4, 60, 66, 66, 124, 64, 66, 120, 120, 64,
    126, 16, 4, 96, 64, 146, 82, 66, 66, 66, 66, 60, 16, 66, 66, 130, 24, 40, 8, 0, 82, 8, 8, 2,
    66, 2, 66, 8, 66, 62, 126, 66, 64, 66, 64, 64, 78, 66, 16, 4, 96, 64, 130, 74, 66, 124, 66,
    124, 2, 16, 66, 36, 146, 24, 16, 16, 0, 98, 8, 16, 2, 126, 2, 66, 8, 66, 2, 66, 66, 64, 66, 64,
    64, 66, 66, 16, 4, 80, 64, 130, 70, 66, 64, 66, 80, 2, 16, 66, 36, 146, 36, 16, 32, 0, 66, 8,
    32, 66, 2, 2, 66, 16, 66, 2, 66, 66, 66, 66, 64, 64, 66, 66, 16, 68, 72, 64, 130, 66, 66, 64,
    66, 72, 66, 16, 66, 36, 170, 36, 16, 64, 0, 66, 8, 64, 66, 2, 66, 66, 16, 66, 4, 66, 66, 66,
    68, 64, 64, 66, 66, 16, 68, 68, 64, 130, 66, 66, 64, 74, 68, 66, 16, 66, 24, 198, 66, 16, 64,
    0, 60, 62, 126, 60, 2, 60, 60, 16, 60, 56, 66, 124, 60, 120, 126, 64, 60, 66, 56, 56, 66, 126,
    130, 66, 60, 64, 60, 66, 60, 16, 60, 24, 130, 66, 16, 126, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub const FROZEN_2048_1392: [u32; 64] = [
    0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0x7fffffff, 0x11f7fff,
    0xffffffff, 0x7fffffff, 0x17ffffff, 0x117177f, 0x177f7fff, 0x1037f, 0x1011f, 0x1, 0xffffffff,
    0x177fffff, 0x77f7fff, 0x1011f, 0x1173fff, 0x10117, 0x10117, 0x0, 0x117177f, 0x17, 0x3, 0x0,
    0x1, 0x0, 0x0, 0x0, 0x7fffffff, 0x11f7fff, 0x11717ff, 0x117, 0x17177f, 0x3, 0x1, 0x0, 0x1037f,
    0x1, 0x1, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1011f, 0x1, 0x1, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0, 0x0,
];
pub const FROZEN_2048_1056: [u32; 64] = [
    0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0x7fffffff,
    0xffffffff, 0xffffffff, 0xffffffff, 0x7fffffff, 0xffffffff, 0x177fffff, 0x177f7fff, 0x1017f,
    0xffffffff, 0xffffffff, 0xffffffff, 0x177f7fff, 0x7fffffff, 0x13f7fff, 0x1171fff, 0x117,
    0x3fffffff, 0x11717ff, 0x7177f, 0x1, 0x1017f, 0x1, 0x1, 0x0, 0xffffffff, 0x7fffffff,
    0x7fffffff, 0x1171fff, 0x17ffffff, 0x7177f, 0x1037f, 0x1, 0x77f7fff, 0x1013f, 0x10117, 0x1,
    0x10117, 0x0, 0x0, 0x0, 0x1173fff, 0x10117, 0x117, 0x0, 0x7, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0,
    0x0, 0x0, 0x0, 0x0,
];
pub const FROZEN_2048_712: [u32; 64] = [
    0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff,
    0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0x177fffff,
    0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0xffffffff, 0x7fffffff, 0x11f7fff,
    0xffffffff, 0x7fffffff, 0x1fffffff, 0x17177f, 0x177fffff, 0x1037f, 0x1011f, 0x1, 0xffffffff,
    0xffffffff, 0xffffffff, 0x7fffffff, 0xffffffff, 0x1fffffff, 0x177fffff, 0x1077f, 0xffffffff,
    0x177f7fff, 0x13f7fff, 0x10117, 0x1171fff, 0x117, 0x7, 0x0, 0x7fffffff, 0x1173fff, 0x11717ff,
    0x7, 0x3077f, 0x1, 0x1, 0x0, 0x1013f, 0x1, 0x1, 0x0, 0x1, 0x0, 0x0, 0x0,
];

