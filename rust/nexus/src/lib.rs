//! NexusMods update checker for Witcher3StringEditor.
//! Port of Witcher3StringEditor.Services.CheckUpdateService and NexusIntegration.
#![forbid(unsafe_code)]

use std::time::Duration;

use reqwest::Client;
use serde_json::{Value, json};

pub const NEXUS_MOD_URL: &str = "https://www.nexusmods.com/witcher3/mods/7918";
pub const GRAPHQL_URL: &str = "https://api.nexusmods.com/v2/graphql";

pub fn parse_version(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let lat = parse_version(latest);
    let cur = parse_version(current);
    lat > cur
}

pub async fn check_update(current_version: &str) -> Result<Option<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Witcher3StringEditor-Rust/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let payload = json!({
        "query": r#"query GetModVersion {
            mods(filter: { name: [{ op: EQUALS, value: "The Witcher3 String Editor NextGen" }] }) {
                nodes {
                    version
                }
            }
        }"#
    });

    let resp = client
        .post(GRAPHQL_URL)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Nexus API returned status {}", resp.status()));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response JSON: {e}"))?;

    let latest_version = body
        .pointer("/data/mods/nodes/0/version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Could not locate version in response".to_string())?;

    if is_newer_version(latest_version, current_version) {
        Ok(Some(latest_version.to_string()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(is_newer_version("1.2.0", "1.1.0"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.1"));
        assert!(is_newer_version("1.0.0.1", "1.0.0"));
    }
}
