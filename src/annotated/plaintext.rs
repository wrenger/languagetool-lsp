use std::ops::Range;

use anyhow::anyhow;

use crate::source::SourceFile;

use super::AnnotatedText;

pub fn annotate(
    source: &SourceFile,
    mut lines: Range<usize>,
) -> anyhow::Result<(Range<usize>, AnnotatedText)> {
    // Skip whitespace
    if let Some((range, text)) = source.line_range(lines.clone()) {
        if text.trim().is_empty() {
            return Ok((range.0.byte..range.1.byte, AnnotatedText::new()));
        }
    }

    // Find start of paragraph
    for i in (0..lines.start).rev() {
        let line = source
            .line_range(i..i + 1)
            .ok_or_else(|| anyhow!("Invalid Line"))?
            .1;
        if line.trim().is_empty() {
            break;
        }
        lines.start = i;
    }
    // Find end of paragraph
    for i in lines.end..source.lines().len() {
        let line = source
            .line_range(i..i + 1)
            .ok_or_else(|| anyhow!("Invalid Line"))?
            .1;
        if line.trim().is_empty() {
            break;
        }
        lines.end = i;
    }

    let (range, text) = source
        .line_range(lines.clone())
        .ok_or_else(|| anyhow!("Invalid Line"))?;

    let mut annot = AnnotatedText::new();
    if !text.trim().is_empty() {
        annot.add_text(text.to_string());
    }
    Ok((range.0.byte..range.1.byte, annot))
}
