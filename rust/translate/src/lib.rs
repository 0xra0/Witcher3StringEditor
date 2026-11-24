//! Translation providers for Witcher3StringEditor:
//! GoogleTranslator, MicrosoftTranslator, YandexTranslator.
#![forbid(unsafe_code)]

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

pub const TRANSLATOR_NAMES: &[&str] = &[
    "MicrosoftTranslator",
    "GoogleTranslator",
    "YandexTranslator",
];

#[derive(Clone)]
pub struct TranslationClient {
    http: Client,
}

impl Default for TranslationClient {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslationClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default();
        Self { http }
    }

    pub async fn translate(
        &self,
        translator_name: &str,
        text: &str,
        from_lang: &str,
        to_lang: &str,
    ) -> Result<String, String> {
        if text.trim().is_empty() {
            return Ok(text.to_string());
        }

        match translator_name {
            "GoogleTranslator" => self.translate_google(text, from_lang, to_lang).await,
            "MicrosoftTranslator" => self.translate_microsoft(text, from_lang, to_lang).await,
            "YandexTranslator" => self.translate_yandex(text, from_lang, to_lang).await,
            _ => Err(format!("Unknown translation provider: {translator_name}")),
        }
    }

    pub async fn translate_google(
        &self,
        text: &str,
        from_lang: &str,
        to_lang: &str,
    ) -> Result<String, String> {
        let from = if from_lang.is_empty() { "auto" } else { from_lang };
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl={from}&tl={to_lang}&dt=t&q={}",
            urlencoding(text)
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Google translate request failed: {}", e.without_url()))?;

        if !resp.status().is_success() {
            return Err(format!("Google translate returned status {}", resp.status()));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        parse_google_response(&body)
    }

    pub async fn translate_microsoft(
        &self,
        text: &str,
        from_lang: &str,
        to_lang: &str,
    ) -> Result<String, String> {
        // Bing translation endpoint
        let url = "https://www.bing.com/ttranslatev3";
        let mut form = vec![
            ("text", text),
            ("to", to_lang),
        ];
        if !from_lang.is_empty() && from_lang != "auto" {
            form.push(("fromLang", from_lang));
        }

        let response = self
            .http
            .post(url)
            .form(&form)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("Microsoft translation request failed: {e}"))?;
        let json: Value = response.json().await
            .map_err(|e| format!("Invalid Microsoft translation response: {e}"))?;
        json.pointer("/0/translations/0/text")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "No translation text in Microsoft response".into())
    }

    pub async fn translate_yandex(
        &self,
        _text: &str,
        _from_lang: &str,
        _to_lang: &str,
    ) -> Result<String, String> {
        Err("Yandex translation is unavailable. Select another provider in Settings.".into())
    }
}

fn parse_google_response(body: &str) -> Result<String, String> {
    let json: Value = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse Google translate response: {e}"))?;

    let Some(first_arr) = json.get(0).and_then(|v| v.as_array()) else {
        return Err("Unexpected Google response format".into());
    };

    let mut result = String::new();
    for chunk in first_arr {
        if let Some(str_val) = chunk.get(0).and_then(|v| v.as_str()) {
            result.push_str(str_val);
        }
    }

    if result.is_empty() {
        Err("No translation text extracted from Google response".into())
    } else {
        Ok(result)
    }
}

fn urlencoding(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_providers_return_an_error() {
        let client = TranslationClient::new();
        assert!(client.translate("YandexTranslator", "test", "en", "pl")
            .await.unwrap_err().contains("Yandex translation is unavailable"));
        assert!(client.translate("Unknown", "test", "en", "pl")
            .await.unwrap_err().contains("Unknown translation provider"));
    }

    #[test]
    fn test_parse_google_response() {
        let mock_body = r#"[[["Cześć świecie","Hello world",null,null,1]],null,"en"]"#;
        let res = parse_google_response(mock_body).unwrap();
        assert_eq!(res, "Cześć świecie");
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding("hello world!"), "hello%20world%21");
    }
}
