use std::ops::Range;

use anyhow::{Result, anyhow};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use super::handle_response_errors;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Synonyms {
    En,
    De,
}

impl Synonyms {
    pub async fn query(self, line: &str, selection: Range<usize>) -> Result<Vec<String>> {
        let sentence_start = line[..selection.start].rfind(".").unwrap_or(0);
        let sentence_end = line[selection.end..]
            .find(".")
            .map(|i| selection.end + i)
            .unwrap_or(line.len());

        let sentence = line[sentence_start..sentence_end].trim();

        match self {
            Synonyms::En => synonyms_en(sentence, selection).await,
            Synonyms::De => synonyms_de(sentence, selection).await,
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Synonyms::En => "https://qb-grammar-en.languagetool.org/phrasal-paraphraser/subscribe/",
            Synonyms::De => "https://synonyms.languagetool.org/synonyms/de/",
        }
    }
}

async fn synonyms_en(sentence: &str, selection: Range<usize>) -> Result<Vec<String>> {
    let index = sentence[0..selection.start].split_whitespace().count();
    let word = sentence[selection.clone()].trim();

    let body = serde_json::json!({
        "message": {
            "indices": [index],
            "mode": 0,
            "phrases": [word],
            "text": sentence,
        },
        "meta": {
            "clientStatus": "string",
            "product": "string",
            "traceID": "string",
            "userID": "string",
        },
        "response_queue": "string",
    });

    let client = reqwest::Client::new();
    let response = client
        .post(Synonyms::En.url())
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?;
    let response = handle_response_errors(response).await?;

    let data = response.json::<serde_json::Value>().await?;
    let synonyms = || -> Option<Vec<String>> {
        Some(
            data.get("data")?
                .get("suggestions")?
                .as_object()?
                .values()
                .filter_map(|v| v.as_array())
                .flat_map(|v| v.iter().filter_map(|v| v.as_str()))
                .map(|v| v.to_string())
                .collect::<Vec<String>>(),
        )
    }()
    .ok_or_else(|| anyhow!("Invalid response"))?;

    Ok(synonyms)
}

async fn synonyms_de(sentence: &str, selection: Range<usize>) -> Result<Vec<String>> {
    let word = sentence[selection.clone()].trim();
    if word.contains(char::is_whitespace) {
        return Err(anyhow!("Word contains whitespace"));
    }
    let before = sentence[..selection.start]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let after = sentence[selection.end..]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let mut url = Url::parse(Synonyms::De.url())?.join(word)?;
    url.query_pairs_mut()
        .append_pair("before", &before)
        .append_pair("after", &after);

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await?;

    let data: serde_json::Value = handle_response_errors(response).await?.json().await?;

    let synonyms = || -> Option<Vec<String>> {
        Some(
            data.get("synsets")?
                .as_array()?
                .iter()
                .filter_map(|s| {
                    Some(
                        s.get("terms")?
                            .as_array()?
                            .iter()
                            .filter_map(|t| t.get("term")?.as_str()),
                    )
                })
                .flatten()
                .map(|t| t.to_string())
                .collect::<Vec<String>>(),
        )
    }()
    .ok_or_else(|| anyhow!("Invalid response"))?;

    Ok(synonyms)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn url() {
        let url = Url::parse(Synonyms::De.url()).unwrap().join("baz").unwrap();
        let url =
            Url::parse_with_params(url.as_str(), &[("before", "foo"), ("after", "bar")]).unwrap();
        println!("{url}")
    }

    #[ignore]
    #[tokio::test]
    async fn en() {
        let sentence = "This is a test sentence.";
        let selection = 10..14;
        let synonyms = Synonyms::En.query(sentence, selection).await.unwrap();
        println!("{synonyms:?}");
    }

    #[ignore]
    #[tokio::test]
    async fn de() {
        let sentence = "Dies ist ein Test Satz.";
        let selection = 13..17;
        let synonyms = Synonyms::En.query(sentence, selection).await.unwrap();
        println!("{synonyms:?}");
    }
}
