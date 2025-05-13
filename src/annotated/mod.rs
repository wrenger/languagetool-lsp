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
    pub fn optimize(&mut self) {
        // TODO: remove whitespace from start/end

        for old in std::mem::take(&mut self.annotation) {
            match (old, self.annotation.last_mut()) {
                (Annotation::Text { text }, Some(Annotation::Text { text: last_text })) => {
                    last_text.push_str(&text);
                }
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
                    last_markup.push_str(&markup);
                }
                (old, _) => self.annotation.push(old),
            }
        }
    }
    pub fn len(&self) -> usize {
        self.annotation.iter().map(|a| match a {
            Annotation::Text { text } => text.len(),
            Annotation::Markup { markup, .. } => markup.len(),
        }).sum()
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
