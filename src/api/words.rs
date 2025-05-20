use crate::settings::Settings;

use anyhow::anyhow;

use super::handle_response_errors;

pub async fn get(settings: &Settings) -> anyhow::Result<Vec<String>> {
    if settings.username.is_empty() || settings.api_key.is_empty() {
        return Err(anyhow!("Syncing words is only supported for premium users"));
    }

    let url = settings.server.join("v2/words")?;
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .query(&[
            ("username", settings.username.as_str()),
            ("apiKey", settings.api_key.as_str()),
            ("limit", "1000"),
        ])
        .send()
        .await?;
    let response = handle_response_errors(response).await?;

    #[derive(serde::Deserialize)]
    struct WordsResponse {
        words: Vec<String>,
    }
    let data: WordsResponse = response.json().await?;
    Ok(data.words)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WordRequest<'a> {
    word: &'a str,
    username: &'a str,
    api_key: &'a str,
}

pub async fn add(settings: &Settings, word: &str) -> anyhow::Result<bool> {
    if settings.username.is_empty() || settings.api_key.is_empty() {
        return Err(anyhow!("Syncing words is only supported for premium users"));
    }

    let url = settings.server.join("v2/words/add")?;
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .form(&WordRequest {
            word: &word,
            username: &settings.username,
            api_key: &settings.api_key,
        })
        .send()
        .await?;
    let response = handle_response_errors(response).await?;
    let data: serde_json::Value = response.json().await?;
    let success = data.get("added").and_then(|s| s.as_bool()).unwrap_or(false);

    Ok(success)
}

pub async fn delete(settings: &Settings, word: &str) -> anyhow::Result<bool> {
    if settings.username.is_empty() || settings.api_key.is_empty() {
        return Err(anyhow!("Syncing words is only supported for premium users"));
    }

    let url = settings.server.join("v2/words/delete")?;
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .form(&WordRequest {
            word: &word,
            username: &settings.username,
            api_key: &settings.api_key,
        })
        .send()
        .await?;
    let response = handle_response_errors(response).await?;
    let data: serde_json::Value = response.json().await?;
    let success = data
        .get("deleted")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    Ok(success)
}
