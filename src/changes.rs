use std::ops::Range;

pub struct Changes {
    changes: Vec<Range<usize>>,
}

impl Changes {
    pub fn new() -> Self {
        Self {
            changes: Vec::new(),
        }
    }

    pub fn add_change(&mut self, range: Range<usize>, len: usize) {
        // Shift existing ranges
        let shift = len as isize - range.len() as isize;
        for change in &mut self.changes {
            if change.start >= range.end {
                change.start = usize::try_from(change.start as isize + shift).unwrap();
            }
            if change.end >= range.end {
                change.end = usize::try_from(change.end as isize + shift).unwrap();
            }
        }
        // Add new range
        self.changes.push(range.start..usize::try_from(range.end as isize + shift).unwrap());

        // Merge overlapping ranges
        self.changes.sort_by(|a, b| a.start.cmp(&b.start));
        let mut merged = Vec::new();
        let mut last: Option<&mut Range<usize>> = None;
        for change in &self.changes {
            if let Some(last) = &mut last {
                if change.start <= last.end {
                    last.start = last.start.min(change.start);
                    last.end = last.end.max(change.end);
                    continue;
                }
            }
            merged.push(change.clone());
            last = merged.last_mut();
        }
        self.changes = merged;
    }

    pub fn changes(&self) -> &Vec<Range<usize>> {
        &self.changes
    }

    pub fn clear(&mut self) {
        self.changes.clear();
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn changes() {
        use super::*;

        let mut changes = Changes::new();
        changes.add_change(0..5, 3);
        assert_eq!(changes.changes(), &vec![0..3]);
        changes.add_change(3..7, 4);
        assert_eq!(changes.changes(), &vec![0..7]);
        changes.add_change(20..30, 3);
        assert_eq!(changes.changes(), &vec![0..7, 20..23]);
        changes.add_change(0..1, 0);
        assert_eq!(changes.changes(), &vec![0..6, 19..22]);
        changes.add_change(0..0, 10);
        assert_eq!(changes.changes(), &vec![0..16, 29..32]);
    }
}
