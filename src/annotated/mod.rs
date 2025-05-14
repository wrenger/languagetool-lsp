use serde::Serialize;

pub mod plaintext;

/// Represents a text with annotations for LanguageTool.
#[derive(Serialize, Debug, Clone)]
pub struct AnnotatedText {
    annotation: Vec<Annotation>,
}

impl AnnotatedText {
    pub fn new() -> Self {
        Self {
            annotation: Vec::new(),
        }
    }
    pub fn add_text(&mut self, text: String) {
        self.annotation.push(Annotation::Text { text });
    }
    pub fn add_markup(&mut self, markup: String, interpret_as: String) {
        self.annotation.push(Annotation::Markup {
            markup,
            interpret_as,
        });
    }
    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.annotation.iter().map(|a| match a {
            Annotation::Text { text } => text.as_str(),
            Annotation::Markup { markup, .. } => markup.as_str(),
        })
    }
    pub fn optimize(&mut self) -> usize {
        let mut offset = 0;
        for old in std::mem::take(&mut self.annotation) {
            match (old, self.annotation.last_mut()) {
                // Remove whitespace from start
                (Annotation::Text { text }, None) if text.trim().is_empty() => offset += text.len(),
                (
                    Annotation::Markup {
                        markup,
                        interpret_as,
                    },
                    None,
                ) if interpret_as.trim().is_empty() => offset += markup.len(),
                // Concatenate text
                (Annotation::Text { text }, Some(Annotation::Text { text: last_text })) => {
                    last_text.push_str(&text)
                }
                // Concatenate markup
                (
                    Annotation::Markup {
                        markup,
                        interpret_as,
                    },
                    Some(Annotation::Markup {
                        markup: last_markup,
                        interpret_as: last_interpret_as,
                    }),
                ) if interpret_as.is_empty() && last_interpret_as.is_empty() => {
                    last_markup.push_str(&markup)
                }
                (old, _) => self.annotation.push(old),
            }
        }
        // Remove whitespace from end
        while let Some(annot) = self.annotation.last_mut() {
            match annot {
                Annotation::Text { text }
                | Annotation::Markup {
                    interpret_as: text, ..
                } => {
                    let trimmed = text.trim_end();
                    if trimmed.is_empty() {
                        self.annotation.pop();
                    } else {
                        text.truncate(trimmed.len());
                        break;
                    }
                }
            }
        }
        offset
    }
    pub fn len(&self) -> usize {
        self.annotation
            .iter()
            .map(|a| match a {
                Annotation::Text { text } => text.len(),
                Annotation::Markup { markup, .. } => markup.len(),
            })
            .sum()
    }
}

/// Represents a range of text in the source document.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase", untagged)]
enum Annotation {
    Text {
        text: String,
    },
    Markup {
        markup: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        interpret_as: String,
    },
}
