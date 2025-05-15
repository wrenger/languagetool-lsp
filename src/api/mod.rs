use anyhow::anyhow;
use std::ops::Range;
use tracing::error;

mod check;
pub use check::check;
mod synonyms;
pub use synonyms::Synonyms;

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

async fn handle_response_errors(response: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    if !response.status().is_success() {
        error!("Response: {response:?}");
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
    Ok(response)
}
