use std::ops::Range;

pub fn utf16_to_byte(chars: impl Iterator<Item = char>, utf16_pos: usize) -> usize {
    let mut byte_offset = 0;
    let mut utf16_offset = 0;
    for c in chars {
        if utf16_offset >= utf16_pos {
            break;
        }
        byte_offset += c.len_utf8();
        utf16_offset += c.len_utf16();
    }
    byte_offset
}


pub trait RangeExt {
    fn touches(&self, other: &Self) -> bool;
}

impl RangeExt for Range<usize> {
    fn touches(&self, other: &Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}
