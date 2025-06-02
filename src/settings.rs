use std::collections::HashMap;

use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::api::Synonyms;

const ENDPOINTS: [Endpoint; 3] = [
    Endpoint::new("https://api.languagetool.org", 20.0, 20000),
    Endpoint::new("https://api.languagetoolplus.com", 80.0, 75000),
    Endpoint::new("", 120.0, 1000000),
];

pub struct Endpoint {
    url: &'static str,
    requests_per_s: f64,
    max_size: usize,
}
impl Endpoint {
    pub const fn new(url: &'static str, requests_per_s: f64, max_size: usize) -> Self {
        Self {
            url,
            requests_per_s,
            max_size,
        }
    }
    pub const fn min_delay(&self) -> f64 {
        (60.0 / self.requests_per_s) * 1000.0
    }
}

/// Settings for the LanguageTool server
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    #[serde(with = "serde_url")]
    pub server: Url,
    pub api_key: String,
    pub username: String,

    pub auto_check: bool,
    pub auto_check_delay: f64,
    pub synonyms: Synonyms,

    pub mother_tongue: String,
    pub static_language: Option<String>,
    pub language_variety: HashMap<String, String>,

    pub dictionary: Vec<String>,
    pub sync_dictionary: bool,
    /// Snapshot of the last synchronization
    pub remote_dictionary: Vec<String>,

    pub picky: bool,
    pub enabled_categories: String,
    pub disabled_categories: String,
    pub enabled_rules: Vec<String>,
    pub disabled_rules: Vec<String>,
}

mod serde_url {
    use reqwest::Url;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
    pub fn deserialize<'de, D: Deserializer<'de>>(val: D) -> Result<Url, D::Error> {
        let s = String::deserialize(val)?;
        if s.is_empty() {
            return Ok(super::ENDPOINTS[0].url.parse().unwrap());
        }
        Url::parse(&s).map_err(|e| Error::custom(format!("invalid URL: {e}")))
    }
    pub fn serialize<S: Serializer>(val: &Url, ser: S) -> Result<S::Ok, S::Error> {
        String::from(val.clone()).serialize(ser)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ENDPOINTS[0].url.parse().unwrap(),
            api_key: String::new(),
            username: String::new(),
            auto_check: true,
            auto_check_delay: ENDPOINTS[0].min_delay(),
            synonyms: Synonyms::En,
            mother_tongue: String::new(),
            static_language: None,
            language_variety: [
                ("en".to_string(), "en-US".to_string()),
                ("de".to_string(), "de-DE".to_string()),
                ("pt".to_string(), "pt-PT".to_string()),
                ("ca".to_string(), "ca-ES".to_string()),
            ]
            .into(),
            dictionary: Vec::new(),
            sync_dictionary: false,
            remote_dictionary: Vec::new(),
            picky: false,
            enabled_categories: String::new(),
            disabled_categories: String::new(),
            enabled_rules: Vec::new(),
            disabled_rules: Vec::new(),
        }
    }
}
