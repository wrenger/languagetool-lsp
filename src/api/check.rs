use serde::{Deserialize, Serialize};
use tracing::info;

use crate::annotated::AnnotatedText;
use crate::api::handle_response_errors;
use crate::settings::Settings;
use crate::util::utf16_to_byte;

use super::Match;

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

    let url = settings.server.join("v2/check")?;
    info!("url: {url}");
    let client = reqwest::Client::new();
    let response = client.post(url).form(&params).send().await?;
    let response = handle_response_errors(response).await?;

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
