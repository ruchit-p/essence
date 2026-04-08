use crate::{
    api::scrape::scrape_core_logic,
    crawler::mapper,
    error::ScrapeError,
    types::{LlmsTxtRequest, LlmsTxtResponse, MapRequest, ScrapeRequest},
    validation,
};
use axum::Json;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// A processed page ready for llms.txt output.
struct ProcessedPage {
    url: String,
    title: String,
    description: String,
    markdown: String,
}

/// Handler for POST /api/v1/llmstxt
pub async fn llmstxt_handler(
    Json(request): Json<LlmsTxtRequest>,
) -> Result<Json<LlmsTxtResponse>, ScrapeError> {
    let response = llmstxt_core_logic(&request).await?;
    Ok(Json(response))
}

/// Core logic for generating llms.txt, callable from both API handler and MCP.
pub async fn llmstxt_core_logic(request: &LlmsTxtRequest) -> Result<LlmsTxtResponse, ScrapeError> {
    info!(
        "llms.txt generation requested for URL: {} (max_urls: {})",
        request.url, request.max_urls
    );

    // Validate the request
    validation::validate_llmstxt_request(request).await?;

    // Step 1: Discover URLs using the map functionality.
    // Discover more URLs than needed, then prioritize same-host URLs so we
    // get the actual docs pages rather than external/marketing links.
    let parsed_input = url::Url::parse(&request.url).ok();
    let input_host = parsed_input
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("");

    let map_request = MapRequest {
        url: request.url.clone(),
        search: None,
        ignore_sitemap: request.ignore_sitemap,
        include_subdomains: request.include_subdomains.or(Some(true)),
        limit: Some(request.max_urls * 10), // discover more, filter later
    };

    let mut urls = mapper::discover_urls(&request.url, &map_request)
        .await
        .map_err(|e| {
            error!("Failed to discover URLs for {}: {}", request.url, e);
            e
        })?;

    if urls.is_empty() {
        return Err(ScrapeError::EmptyContent(
            "No URLs found for the website".to_string(),
        ));
    }

    // Prioritize URLs from the same host as the input URL.
    // This ensures docs.example.com/docs pages come before unrelated marketing pages.
    urls.sort_by(|a, b| {
        let a_same = url::Url::parse(a)
            .ok()
            .and_then(|u| u.host_str().map(|h| h == input_host))
            .unwrap_or(false);
        let b_same = url::Url::parse(b)
            .ok()
            .and_then(|u| u.host_str().map(|h| h == input_host))
            .unwrap_or(false);
        b_same.cmp(&a_same).then(a.cmp(b))
    });

    let urls_total = urls.len();
    let urls: Vec<String> = urls.into_iter().take(request.max_urls as usize).collect();

    info!(
        "Discovered {} URLs, processing up to {}",
        urls_total,
        urls.len()
    );

    // Step 2: Scrape all URLs concurrently with a semaphore
    let semaphore = Arc::new(Semaphore::new(request.max_concurrent_scrapes as usize));
    let llm_config = request.llm_base_url.as_ref().map(|base_url| LlmConfig {
        base_url: base_url.clone(),
        model: request
            .llm_model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string()),
        api_key: request.llm_api_key.clone(),
    });
    let llm_config = Arc::new(llm_config);
    let http_client = Arc::new(
        Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ScrapeError::Internal(format!("Failed to create HTTP client: {}", e)))?,
    );

    let mut tasks = Vec::new();

    let engine = request.engine.clone();
    for url in &urls {
        let url = url.clone();
        let sem = semaphore.clone();
        let llm_cfg = llm_config.clone();
        let client = http_client.clone();
        let engine = engine.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            process_url(&url, llm_cfg.as_ref().as_ref(), &client, &engine).await
        }));
    }

    let results = futures::future::join_all(tasks).await;

    let mut pages: Vec<ProcessedPage> = Vec::new();
    for result in results {
        match result {
            Ok(Some(page)) => pages.push(page),
            Ok(None) => {} // skipped
            Err(e) => warn!("Task join error: {}", e),
        }
    }

    if pages.is_empty() {
        return Err(ScrapeError::EmptyContent(
            "No pages could be processed".to_string(),
        ));
    }

    info!(
        "Successfully processed {}/{} pages",
        pages.len(),
        urls.len()
    );

    // Step 3: Build llms.txt and llms-full.txt output
    let llmstxt = build_llmstxt(&request.url, &pages);
    let llms_fulltxt = if request.show_full_text {
        Some(build_llms_fulltxt(&request.url, &pages))
    } else {
        None
    };

    Ok(LlmsTxtResponse::success(
        llmstxt,
        llms_fulltxt,
        pages.len(),
        urls_total,
    ))
}

/// Process a single URL: scrape and extract title/description.
/// Uses HTTP first; if the result is thin and engine is "auto", retries with browser fallback.
async fn process_url(
    url: &str,
    llm_config: Option<&LlmConfig>,
    http_client: &Client,
    engine: &str,
) -> Option<ProcessedPage> {
    debug!("Processing URL: {}", url);

    let scrape_request = ScrapeRequest {
        url: url.to_string(),
        formats: vec!["markdown".to_string()],
        engine: "http".to_string(), // Always try HTTP first (fast)
        only_main_content: true,
        timeout: 15000,
        ..ScrapeRequest::default()
    };

    let http_result = scrape_core_logic(&scrape_request).await;

    // Determine if we need browser fallback:
    // - HTTP returned EmptyContent/LowQuality error (SPA shell)
    // - HTTP returned thin markdown (< 100 chars)
    let needs_fallback = engine == "auto"
        && match &http_result {
            Err(crate::error::ScrapeError::EmptyContent(_))
            | Err(crate::error::ScrapeError::LowQuality(_)) => true,
            Ok(r) => r
                .data
                .as_ref()
                .and_then(|d| d.markdown.as_ref())
                .is_none_or(|m| m.trim().len() < 100),
            _ => false,
        };

    let data = if needs_fallback {
        debug!(
            "HTTP returned thin/empty content for {}, retrying with auto engine (browser fallback)",
            url
        );
        let auto_request = ScrapeRequest {
            engine: "auto".to_string(),
            timeout: 30000,
            ..scrape_request
        };
        match scrape_core_logic(&auto_request).await {
            Ok(r) => match r.data {
                Some(d) => d,
                None => {
                    // Auto also failed — try to use original HTTP result if any
                    match http_result {
                        Ok(r) => r.data?,
                        Err(_) => return None,
                    }
                }
            },
            Err(e) => {
                warn!("Auto engine also failed for {}: {}", url, e);
                match http_result {
                    Ok(r) => r.data?,
                    Err(_) => return None,
                }
            }
        }
    } else {
        match http_result {
            Ok(r) => r.data?,
            Err(e) => {
                warn!("Failed to scrape {}: {}", url, e);
                return None;
            }
        }
    };

    let markdown = data.markdown.unwrap_or_default();
    if markdown.is_empty() {
        debug!("Skipping {} - empty markdown", url);
        return None;
    }

    // Check status code
    if data.metadata.status_code >= 400 {
        debug!("Skipping {} - HTTP {}", url, data.metadata.status_code);
        return None;
    }

    // Extract title
    let title = data
        .metadata
        .title
        .as_deref()
        .or(data.metadata.og_title.as_deref())
        .or(data.title.as_deref())
        .unwrap_or_else(|| url.split('/').next_back().unwrap_or("Untitled"))
        .to_string();

    // Detect soft 404s
    let title_lower = title.to_lowercase();
    if title_lower.contains("not found") || title_lower.contains("404") {
        debug!("Skipping soft 404: {} (title: {})", url, title);
        return None;
    }

    if let Some(robots) = &data.metadata.robots {
        if robots.starts_with("noindex") {
            debug!("Skipping noindex page: {}", url);
            return None;
        }
    }

    // Generate description
    let description = if let Some(config) = llm_config {
        match generate_llm_description(http_client, config, url, &markdown).await {
            Ok(desc) => desc,
            Err(e) => {
                warn!("LLM description failed for {}: {}, using metadata", url, e);
                get_metadata_description(&data.metadata, &data.description)
            }
        }
    } else {
        get_metadata_description(&data.metadata, &data.description)
    };

    Some(ProcessedPage {
        url: url.to_string(),
        title,
        description,
        markdown,
    })
}

/// Extract a description from page metadata.
fn get_metadata_description(
    metadata: &crate::types::Metadata,
    doc_description: &Option<String>,
) -> String {
    metadata
        .description
        .as_deref()
        .or(metadata.og_description.as_deref())
        .or(metadata.excerpt.as_deref())
        .or(doc_description.as_deref())
        .unwrap_or("No description available")
        .chars()
        .take(150)
        .collect()
}

/// Configuration for an OpenAI-compatible LLM API.
struct LlmConfig {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Clone, Copy)]
enum LlmApiType {
    ChatCompletions,
    Responses,
}

/// Chat completions response shape.
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

/// Call an OpenAI-compatible LLM API to generate a page description.
/// Supports both Chat Completions and Responses API formats.
async fn generate_llm_description(
    client: &Client,
    config: &LlmConfig,
    url: &str,
    markdown: &str,
) -> Result<String, String> {
    let prompt = format!(
        "Generate a concise 9-10 word description for this webpage. \
         Return ONLY a JSON object, no markdown formatting.\n\n\
         URL: {}\nContent: {}\n\n\
         Return exactly: {{\"description\": \"9-10 word description here\"}}",
        url,
        &markdown[..markdown.len().min(1500)]
    );

    let (endpoint, api_type) = resolve_llm_endpoint(&config.base_url);

    let payload = match api_type {
        LlmApiType::Responses => serde_json::json!({
            "model": config.model,
            "input": prompt,
            "max_output_tokens": 500,
            "store": false
        }),
        LlmApiType::ChatCompletions => serde_json::json!({
            "model": config.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.2,
            "max_tokens": 500
        }),
    };

    let mut req = client.post(&endpoint).json(&payload);

    if let Some(key) = &config.api_key {
        req = req.bearer_auth(key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API returned {} - {}", status, body));
    }

    let content = match api_type {
        LlmApiType::Responses => {
            let data: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse Responses API JSON: {}", e))?;

            // Extract text from output array: skip reasoning items,
            // find message items, extract output_text content parts
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
            } else {
                warn!("Responses API: no 'output' array in response");
            }
            text
        }
        LlmApiType::ChatCompletions => {
            let data: ChatCompletionResponse = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse Chat Completions response: {}", e))?;
            data.choices
                .first()
                .map(|c| c.message.content.clone())
                .unwrap_or_default()
        }
    };

    extract_description_from_llm_response(&content)
}

/// Extract the description field from an LLM JSON response,
/// handling markdown code blocks, malformed JSON, and plain text.
fn extract_description_from_llm_response(content: &str) -> Result<String, String> {
    let mut clean = content.trim().to_string();

    if clean.is_empty() {
        return Err("Empty LLM response".to_string());
    }

    debug!("Raw LLM response: {}", &clean[..clean.len().min(300)]);

    // Strip markdown code fences
    if clean.starts_with("```json") {
        clean = clean[7..].to_string();
    } else if clean.starts_with("```") {
        clean = clean[3..].to_string();
    }
    if clean.ends_with("```") {
        clean = clean[..clean.len() - 3].to_string();
    }
    clean = clean.trim().to_string();

    // Try direct JSON parse
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&clean) {
        if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
            return Ok(desc.chars().take(100).collect());
        }
    }

    // Regex fallback for JSON embedded in text
    let re = Regex::new(r#""description"\s*:\s*"([^"]+)""#).unwrap();
    if let Some(caps) = re.captures(&clean) {
        if let Some(m) = caps.get(1) {
            return Ok(m.as_str().chars().take(100).collect());
        }
    }

    // Final fallback: if the response is plain text (no JSON at all),
    // treat the entire response as the description
    if !clean.contains('{') && !clean.is_empty() {
        let plain: String = clean
            .lines()
            .next()
            .unwrap_or(&clean)
            .chars()
            .take(100)
            .collect();
        if !plain.is_empty() {
            debug!("Using plain text LLM response as description");
            return Ok(plain);
        }
    }

    Err("Could not extract description from LLM response".to_string())
}

/// Build the llms.txt index string.
fn build_llmstxt(site_url: &str, pages: &[ProcessedPage]) -> String {
    let mut out = format!("# {}\n\n", site_url);
    for page in pages {
        out.push_str(&format!(
            "- [{}]({}): {}\n",
            page.title, page.url, page.description
        ));
    }
    out
}

/// Build the llms-full.txt string with full markdown content.
fn build_llms_fulltxt(site_url: &str, pages: &[ProcessedPage]) -> String {
    let mut out = format!("# {}\n\n", site_url);
    for (i, page) in pages.iter().enumerate() {
        out.push_str(&format!(
            "## {}\n\nSource: {}\n\n{}\n\n",
            page.title, page.url, page.markdown
        ));
        if i < pages.len() - 1 {
            out.push_str("---\n\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_description_json() {
        let input = r#"{"description": "A guide to using the API endpoints"}"#;
        let result = extract_description_from_llm_response(input).unwrap();
        assert_eq!(result, "A guide to using the API endpoints");
    }

    #[test]
    fn test_extract_description_code_block() {
        let input = "```json\n{\"description\": \"Testing with code blocks\"}\n```";
        let result = extract_description_from_llm_response(input).unwrap();
        assert_eq!(result, "Testing with code blocks");
    }

    #[test]
    fn test_extract_description_truncates() {
        let long = "a".repeat(200);
        let input = format!(r#"{{"description": "{}"}}"#, long);
        let result = extract_description_from_llm_response(&input).unwrap();
        assert_eq!(result.len(), 100);
    }

    #[test]
    fn test_build_llmstxt() {
        let pages = vec![ProcessedPage {
            url: "https://example.com/docs".to_string(),
            title: "Docs".to_string(),
            description: "Documentation for the project".to_string(),
            markdown: "# Docs\nHello".to_string(),
        }];
        let result = build_llmstxt("https://example.com", &pages);
        assert!(result.contains("# https://example.com"));
        assert!(
            result.contains("- [Docs](https://example.com/docs): Documentation for the project")
        );
    }

    #[test]
    fn test_build_llms_fulltxt() {
        let pages = vec![ProcessedPage {
            url: "https://example.com/docs".to_string(),
            title: "Docs".to_string(),
            description: "Documentation".to_string(),
            markdown: "# Docs\nContent here".to_string(),
        }];
        let result = build_llms_fulltxt("https://example.com", &pages);
        assert!(result.contains("## Docs"));
        assert!(result.contains("Source: https://example.com/docs"));
        assert!(result.contains("# Docs\nContent here"));
    }
}
