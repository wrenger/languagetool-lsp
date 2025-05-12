use std::ops::Range;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::annotated::AnnotatedText;
use crate::settings::Settings;
use crate::util::utf16_to_byte;

/// Represents a match (potential issue) found by LanguageTool.
#[derive(Debug, Clone)]
pub struct Match {
    pub range: Range<usize>,
    pub title: String,
    pub message: String,
    pub replacements: Vec<String>,
    pub category: String,
    pub rule: String,
}

pub async fn check(
    text: AnnotatedText,
    offset: usize,
    settings: &Settings,
    language: Option<String>,
) -> anyhow::Result<Vec<Match>> {
    let params = CheckParams {
        data: &serde_json::to_string(&text)?,
        language: language
            .as_deref()
            .and(settings.static_language.as_deref())
            .unwrap_or("auto"),
        username: &settings.username,
        api_key: &settings.api_key,
        level: if settings.picky { "picky" } else { "default" },
        mother_tongue: &settings.mother_tongue,
        enabled_categories: &settings.enabled_categories,
        disabled_categories: &settings.disabled_categories,
        enabled_rule: &settings.enabled_rules,
        disabled_rule: &settings.disabled_rules,
        preferred_variants: &settings
            .language_variety
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(","),
    };

    let mut url = settings.server.clone();
    url.set_path("/v2/check");
    info!("url: {url}");
    let client = reqwest::Client::new();
    let response = client.post(url).form(&params).send().await?;
    info!("Response: {response:?}");

    if !response.status().is_success() {
        if response.status() == reqwest::StatusCode::GATEWAY_TIMEOUT
            || response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
        {
            return Err(anyhow!(
                "Request to LanguageTool timed out. Please try again later."
            ));
        }
        let status = response.status();
        let mut message = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown Error.".to_string());
        message.truncate(300);
        return Err(anyhow!("Status: {status}\n{message}",));
    }

    let response: CheckResponse = response.json().await?;
    info!("Software {:?}", response.software);

    Ok(response
        .matches
        .into_iter()
        .map(|m| Match {
            // Java and JavaScript Strings are UTF-16, so we need to convert them to UTF-8.
            range: offset + utf16_to_byte(text.parts().flat_map(|p| p.chars()), m.offset)
                ..offset + utf16_to_byte(text.parts().flat_map(|p| p.chars()), m.offset + m.length),
            title: m.short_message,
            message: m.message,
            replacements: m
                .replacements
                .into_iter()
                .take(10)
                .map(|r| r.value)
                .collect(),
            category: m.rule.category.id,
            rule: m.rule.id,
        })
        .collect())
}

/// The response structure returned by the LanguageTool check API.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckResponse {
    matches: Vec<CheckMatch>,
    software: serde_json::Value,
}

/// Represents a single match (potential issue) found by LanguageTool.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckMatch {
    /// The full message describing the issue.
    message: String,
    /// A short version of the message, if available.
    short_message: String,
    /// The rule that triggered this match.
    rule: Rule,
    /// Suggested replacements for the issue.
    replacements: Vec<Replacement>,
    /// The offset (in bytes) where the issue starts in the text.
    offset: usize,
    /// The length (in bytes) of the problematic text.
    length: usize,
}

/// Information about the rule that triggered a match.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    id: String,
    category: Category,
}

/// Represents a category of rules in LanguageTool.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Category {
    id: String,
}

/// Represents a suggested replacement for a detected issue.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Replacement {
    value: String,
}

/// Parameters for the LanguageTool check API call.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CheckParams<'a> {
    /// The annotated text to check.
    data: &'a str,
    /// The language code to use for checking (e.g., "en", "de", or "auto").
    language: &'a str,
    /// The username for authentication, if required.
    #[serde(skip_serializing_if = "str::is_empty")]
    username: &'a str,
    /// The API key for authentication, if required.
    #[serde(skip_serializing_if = "str::is_empty")]
    api_key: &'a str,
    /// The checking level ("picky" or "default").
    level: &'a str,
    /// The user's mother tongue, if specified.
    #[serde(skip_serializing_if = "str::is_empty")]
    mother_tongue: &'a str,
    /// Comma-separated list of enabled categories.
    #[serde(skip_serializing_if = "str::is_empty")]
    enabled_categories: &'a str,
    /// Comma-separated list of disabled categories.
    #[serde(skip_serializing_if = "str::is_empty")]
    disabled_categories: &'a str,
    /// Comma-separated list of enabled rules.
    #[serde(skip_serializing_if = "str::is_empty")]
    enabled_rule: &'a str,
    /// Comma-separated list of disabled rules.
    #[serde(skip_serializing_if = "str::is_empty")]
    disabled_rule: &'a str,
    /// Comma-separated list of preferred language variants.
    #[serde(skip_serializing_if = "str::is_empty")]
    preferred_variants: &'a str,
}
