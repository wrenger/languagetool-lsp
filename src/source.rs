use std::ops::{Add, AddAssign, Range, Sub, SubAssign};

use tower_lsp_server::lsp_types::Position;

use crate::util::utf16_to_byte;

#[derive(Debug, Clone)]
pub struct SourceFile {
    text: String,
    /// Line ranges as (start, end)
    lines: Vec<(Size, Size)>,
}
impl SourceFile {
    pub fn new(text: String) -> Self {
        let mut val = Self {
            text,
            lines: Vec::new(),
        };
        val.compute_lines();
        val
    }

    fn compute_lines(&mut self) {
        let mut last = Size::zero();
        // `lines` does not include line endings, so we need to do it manually
        let mut lines = self
            .text
            .lines()
            .skip(1)
            .map(|line| {
                let offset =
                    usize::try_from(unsafe { line.as_ptr().offset_from(self.text.as_ptr()) })
                        .expect("line offset");
                println!("offset {offset}");
                let curr = last.clone();
                last += Size::new(&self.text[last.byte..offset]);
                (curr, last)
            })
            .collect::<Vec<_>>();

        let curr = last.clone();
        last += Size::new(&self.text[last.byte..]);
        lines.push((curr, last));
        // Add the last line (which is ignored by `lines`)
        if self.text.ends_with("\r\n") || self.text.ends_with('\n') {
            lines.push((last, last));
        }
        self.lines = lines;
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn lines(&self) -> &[(Size, Size)] {
        &self.lines
    }
    pub fn line_start(&self, i: usize) -> Option<Size> {
        self.lines.get(i).map(|(start, _)| *start)
    }
    pub fn line_end(&self, i: usize) -> Option<Size> {
        self.lines.get(i).map(|(_, end)| *end)
    }
    pub fn line_range(&self, range: Range<usize>) -> Option<((Size, Size), &str)> {
        let start = self.line_start(range.start)?;
        let end = self.line_end(range.end - 1)?;
        Some(((start, end), &self.text[start.byte..end.byte]))
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        self.text.replace_range(range, text);
        self.compute_lines();
    }

    /// Convert a utf-16 line/column position to a utf-8 byte offset
    #[allow(unused)]
    pub fn to_offset(&self, pos: Position) -> Option<usize> {
        let (l_start, l_end) = self.lines.get(pos.line as usize)?;
        let line = &self.text[l_start.byte..l_end.byte];
        let byte_offset = utf16_to_byte(line.chars(), pos.character as _);
        Some(l_start.byte + byte_offset)
    }

    /// Convert a utf-8 byte offset to a utf-16 line/column position
    pub fn to_position(&self, offset: usize) -> Option<Position> {
        if offset + 1 > self.text.len() {
            return None;
        } else if offset == self.text.len() {
            return Some(Position {
                line: self.lines.len() as _,
                character: self
                    .lines
                    .last()
                    .map(|(s, e)| e.utf16 - s.utf16)
                    .unwrap_or(0) as _,
            });
        }

        let (line, (l_start, _)) = self
            .lines
            .iter()
            .enumerate()
            .find(|(_, (start, end))| (start.byte..end.byte).contains(&offset))?;

        let prefix = &self.text[l_start.byte..offset];
        let character = prefix.encode_utf16().count() as _;

        Some(Position {
            line: line as _,
            character,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    pub byte: usize,
    pub utf16: usize,
}
impl Size {
    pub fn new(s: &str) -> Self {
        Self {
            byte: s.len(),
            utf16: s.encode_utf16().count(),
        }
    }
    pub fn zero() -> Self {
        Self { byte: 0, utf16: 0 }
    }
}
impl Add for Size {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            byte: self.byte + rhs.byte,
            utf16: self.utf16 + rhs.utf16,
        }
    }
}
impl AddAssign for Size {
    fn add_assign(&mut self, rhs: Self) {
        self.byte += rhs.byte;
        self.utf16 += rhs.utf16;
    }
}
impl Sub for Size {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            byte: self.byte - rhs.byte,
            utf16: self.utf16 - rhs.utf16,
        }
    }
}
impl SubAssign for Size {
    fn sub_assign(&mut self, rhs: Self) {
        self.byte -= rhs.byte;
        self.utf16 -= rhs.utf16;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_lines() {
        let file = SourceFile::new("Hello\nWorld\n".into());
        let [a, b, c] = file.lines.try_into().unwrap();
        assert_eq!(a.0.byte, 0);
        assert_eq!(a.1.byte, 6);
        assert_eq!(b.0.byte, 6);
        assert_eq!(b.1.byte, 12);
        assert_eq!(c.0.byte, 12);
        assert_eq!(c.1.byte, 12);

        let file = SourceFile::new("Hello\nWorld\nFoo".into());
        let [a, b, c] = file.lines.try_into().unwrap();
        assert_eq!(a.0.byte, 0);
        assert_eq!(a.1.byte, 6);
        assert_eq!(b.0.byte, 6);
        assert_eq!(b.1.byte, 12);
        assert_eq!(c.0.byte, 12);
        assert_eq!(c.1.byte, 15);

        let file = SourceFile::new("Hello\r\nWorld\r\nFoo".into());
        let [a, b, c] = file.lines.try_into().unwrap();
        assert_eq!(a.0.byte, 0);
        assert_eq!(a.1.byte, 7);
        assert_eq!(b.0.byte, 7);
        assert_eq!(b.1.byte, 14);
        assert_eq!(c.0.byte, 14);
        assert_eq!(c.1.byte, 17);

        let file = SourceFile::new("▲\nWorld\n".into());
        let [a, b, c] = file.lines.try_into().unwrap();
        println!("{:?}", &file.text[a.0.byte..a.1.byte]);
        assert_eq!(a.0.byte, 0);
        assert_eq!(a.1.byte, 4);
        assert_eq!(a.1.utf16, 2);
        assert_eq!(b.0.byte, 4);
        assert_eq!(b.1.byte, 10);
        assert_eq!(b.1.utf16, 8);
        assert_eq!(c.0.byte, 10);
        assert_eq!(c.1.byte, 10);
    }
}
