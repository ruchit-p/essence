use crate::error::{Result, ScrapeError};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

/// Configuration for an OpenAI-compatible LLM API.
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Clone, Copy)]
enum LlmApiType {
    ChatCompletions,
    Responses,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// Detect the API type and build the endpoint URL from the user-supplied base URL.
fn resolve_llm_endpoint(base_url: &str) -> (String, LlmApiType) {
    let base = base_url.trim_end_matches('/');
    if base.contains("/v1/responses") {
        (base.to_string(), LlmApiType::Responses)
    } else if base.ends_with("/v1/chat/completions") {
        (base.to_string(), LlmApiType::ChatCompletions)
    } else {
        let base = base.strip_suffix("/v1").unwrap_or(base);
        (
            format!("{}/v1/chat/completions", base),
            LlmApiType::ChatCompletions,
        )
    }
}

/// Extract structured data from page content using an LLM.
///
/// Sends the page markdown to an OpenAI-compatible API with a prompt asking
/// it to extract data matching the given schema. Returns parsed JSON.
pub async fn extract_with_llm(
    markdown: &str,
    url: &str,
    schema: Option<&serde_json::Value>,
    prompt: Option<&str>,
    config: &LlmConfig,
) -> Result<serde_json::Value> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ScrapeError::Internal(format!("Failed to create HTTP client: {}", e)))?;

    let truncated_content = &markdown[..markdown.len().min(8000)];

    let mut system_parts = vec![
        "You are a structured data extraction assistant.".to_string(),
        "Extract data from the provided web page content and return ONLY valid JSON.".to_string(),
        "Do not include any explanation, markdown formatting, or code blocks - just the raw JSON object.".to_string(),
    ];

    if let Some(s) = schema {
        system_parts.push(format!(
            "The output MUST conform to this JSON schema:\n{}",
            serde_json::to_string_pretty(s).unwrap_or_default()
        ));
    }

    let system_msg = system_parts.join("\n");

    let mut user_parts = Vec::new();
    if let Some(p) = prompt {
        user_parts.push(p.to_string());
    }
    user_parts.push(format!("Page URL: {}", url));
    user_parts.push(format!("Page content:\n{}", truncated_content));

    let user_msg = user_parts.join("\n\n");

    let (endpoint, api_type) = resolve_llm_endpoint(&config.base_url);

    let payload = match api_type {
        LlmApiType::Responses => serde_json::json!({
            "model": config.model,
            "input": format!("{}\n\n{}", system_msg, user_msg),
            "max_output_tokens": 4000,
            "store": false
        }),
        LlmApiType::ChatCompletions => serde_json::json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": system_msg},
                {"role": "user", "content": user_msg}
            ],
            "temperature": 0.1,
            "max_tokens": 4000
        }),
    };

    debug!("Calling LLM extraction API at {}", endpoint);

    let mut req = client.post(&endpoint).json(&payload);
    if let Some(key) = &config.api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ScrapeError::Internal(format!("LLM extraction request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ScrapeError::Internal(format!(
            "LLM API returned {} - {}",
            status, body
        )));
    }

    let content = match api_type {
        LlmApiType::Responses => {
            let data: serde_json::Value = resp.json().await.map_err(|e| {
                ScrapeError::Internal(format!("Failed to parse Responses API JSON: {}", e))
            })?;

            let mut text = String::new();
            if let Some(output) = data.get("output").and_then(|v| v.as_array()) {
                for item in output {
                    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "message" {
                        if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                            for part in content {
                                let part_type =
                                    part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                if part_type == "output_text" {
                                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                                        text.push_str(t);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            text
        }
        LlmApiType::ChatCompletions => {
            let data: ChatCompletionResponse = resp.json().await.map_err(|e| {
                ScrapeError::Internal(format!("Failed to parse Chat Completions response: {}", e))
            })?;
            data.choices
                .first()
                .map(|c| c.message.content.clone())
                .unwrap_or_default()
        }
    };

    // Parse JSON from LLM response, stripping markdown code blocks if present
    let cleaned = content
        .trim()
        .strip_prefix("```json")
        .or_else(|| content.trim().strip_prefix("```"))
        .unwrap_or(content.trim())
        .strip_suffix("```")
        .unwrap_or(content.trim())
        .trim();

    let parsed: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| {
        warn!(
            "LLM returned invalid JSON: {}",
            &content[..content.len().min(200)]
        );
        ScrapeError::Internal(format!("LLM returned invalid JSON: {}", e))
    })?;

    debug!("LLM extraction successful for {}", url);
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_llm_endpoint_chat_completions() {
        let (url, _) = resolve_llm_endpoint("https://api.openai.com/v1/chat/completions");
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_resolve_llm_endpoint_responses() {
        let (url, _) = resolve_llm_endpoint("https://api.anthropic.com/v1/responses");
        assert_eq!(url, "https://api.anthropic.com/v1/responses");
    }

    #[test]
    fn test_resolve_llm_endpoint_base_only() {
        let (url, _) = resolve_llm_endpoint("https://api.openai.com");
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_resolve_llm_endpoint_with_v1() {
        let (url, _) = resolve_llm_endpoint("https://api.openai.com/v1");
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }
}
