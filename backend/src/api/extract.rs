use crate::{
    api::scrape::scrape_core_logic,
    error::ScrapeError,
    format::{
        extract::{extract_with_css, extraction_completeness},
        llm_extract::{extract_with_llm, LlmConfig},
    },
    types::{ExtractRequest, ExtractResponse, ScrapeRequest},
    validation,
};
use axum::Json;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Handler for POST /api/v1/extract
#[utoipa::path(
    post,
    path = "/api/v1/extract",
    request_body = ExtractRequest,
    responses(
        (status = 200, description = "Data extracted successfully", body = ExtractResponse),
        (status = 400, description = "Invalid request"),
    ),
    tag = "Extract"
)]
pub async fn extract_handler(
    Json(request): Json<ExtractRequest>,
) -> Result<Json<ExtractResponse>, ScrapeError> {
    let response = extract_core_logic(&request).await?;
    Ok(Json(response))
}

/// Core extraction logic callable from both API handler and MCP.
pub async fn extract_core_logic(request: &ExtractRequest) -> Result<ExtractResponse, ScrapeError> {
    info!(
        "Extract request received for {} URL(s) in mode: {}",
        request.urls.len(),
        request.mode
    );

    validation::validate_extract_request(request).await?;

    let llm_config = build_llm_config(request);
    let semaphore = Arc::new(Semaphore::new(5));
    let mut results = Vec::new();
    let mut warnings = Vec::new();

    for url in &request.urls {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| ScrapeError::Internal(format!("Semaphore error: {}", e)))?;

        info!("Extracting from URL: {}", url);

        // Step 1: Scrape the page using existing pipeline
        let scrape_request = ScrapeRequest {
            url: url.clone(),
            formats: vec!["markdown".to_string(), "rawHtml".to_string()],
            engine: request.engine.clone(),
            timeout: request.timeout,
            only_main_content: true,
            ..ScrapeRequest::default()
        };

        let scrape_response = match scrape_core_logic(&scrape_request).await {
            Ok(resp) => resp,
            Err(e) => {
                warn!("Failed to scrape {}: {}", url, e);
                warnings.push(format!("Failed to scrape {}: {}", url, e));
                results.push(serde_json::json!({"url": url, "error": e.to_string()}));
                drop(permit);
                continue;
            }
        };

        let data = match scrape_response.data {
            Some(d) => d,
            None => {
                warnings.push(format!("No data returned for {}", url));
                results.push(serde_json::json!({"url": url, "error": "No data returned"}));
                drop(permit);
                continue;
            }
        };

        let html = data.raw_html.as_deref().unwrap_or("");
        let markdown = data.markdown.as_deref().unwrap_or("");

        // Step 2: Extract based on mode
        let extracted = match request.mode.as_str() {
            "css" => extract_css_mode(html, &request.selectors, &request.schema)?,
            "llm" => {
                let config = llm_config.as_ref().ok_or_else(|| {
                    ScrapeError::InvalidRequest(
                        "LLM mode requires llm_base_url and llm_model".to_string(),
                    )
                })?;
                extract_with_llm(
                    markdown,
                    url,
                    request.schema.as_ref(),
                    request.prompt.as_deref(),
                    config,
                )
                .await
                .map_err(|e| {
                    warn!("LLM extraction failed for {}: {}", url, e);
                    e
                })?
            }
            _ => {
                // Auto mode: try CSS first, fall back to LLM
                extract_auto_mode(
                    html,
                    markdown,
                    url,
                    &request.selectors,
                    &request.schema,
                    request.prompt.as_deref(),
                    llm_config.as_ref(),
                )
                .await?
            }
        };

        debug!("Extraction complete for {}", url);
        results.push(extracted);
        drop(permit);
    }

    let response = if warnings.is_empty() {
        ExtractResponse::success(results)
    } else {
        ExtractResponse::success_with_warning(results, warnings.join("; "))
    };

    Ok(response)
}

fn extract_css_mode(
    html: &str,
    selectors: &Option<std::collections::HashMap<String, String>>,
    schema: &Option<serde_json::Value>,
) -> Result<serde_json::Value, ScrapeError> {
    let selectors = selectors.as_ref().ok_or_else(|| {
        ScrapeError::InvalidRequest(
            "CSS mode requires 'selectors' field with CSS selector mappings".to_string(),
        )
    })?;
    extract_with_css(html, selectors, schema.as_ref())
}

async fn extract_auto_mode(
    html: &str,
    markdown: &str,
    url: &str,
    selectors: &Option<std::collections::HashMap<String, String>>,
    schema: &Option<serde_json::Value>,
    prompt: Option<&str>,
    llm_config: Option<&LlmConfig>,
) -> Result<serde_json::Value, ScrapeError> {
    // Try CSS extraction first if selectors provided
    if let Some(sel) = selectors {
        let css_result = extract_with_css(html, sel, schema.as_ref())?;
        let completeness = extraction_completeness(&css_result);
        debug!(
            "CSS extraction completeness for {}: {:.0}%",
            url,
            completeness * 100.0
        );

        if completeness >= 0.5 {
            return Ok(css_result);
        }

        // CSS result too incomplete, try LLM if available
        if let Some(config) = llm_config {
            info!(
                "CSS extraction incomplete ({:.0}%), falling back to LLM for {}",
                completeness * 100.0,
                url
            );
            return extract_with_llm(markdown, url, schema.as_ref(), prompt, config).await;
        }

        // No LLM available, return partial CSS result
        return Ok(css_result);
    }

    // No selectors provided — try LLM if available
    if let Some(config) = llm_config {
        return extract_with_llm(markdown, url, schema.as_ref(), prompt, config).await;
    }

    Err(ScrapeError::InvalidRequest(
        "Auto mode requires either 'selectors' for CSS extraction or LLM credentials (llm_base_url, llm_model) for AI extraction".to_string(),
    ))
}

fn build_llm_config(request: &ExtractRequest) -> Option<LlmConfig> {
    let base_url = request.llm_base_url.as_ref()?;
    Some(LlmConfig {
        base_url: base_url.clone(),
        model: request
            .llm_model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string()),
        api_key: request.llm_api_key.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_llm_config_with_all_fields() {
        let request = ExtractRequest {
            urls: vec!["https://example.com".to_string()],
            schema: None,
            prompt: None,
            selectors: None,
            mode: "llm".to_string(),
            llm_base_url: Some("https://api.openai.com".to_string()),
            llm_model: Some("gpt-4o".to_string()),
            llm_api_key: Some("sk-test".to_string()),
            engine: "auto".to_string(),
            timeout: 30000,
        };

        let config = build_llm_config(&request).unwrap();
        assert_eq!(config.base_url, "https://api.openai.com");
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_key.unwrap(), "sk-test");
    }

    #[test]
    fn test_build_llm_config_without_url_returns_none() {
        let request = ExtractRequest {
            urls: vec!["https://example.com".to_string()],
            schema: None,
            prompt: None,
            selectors: None,
            mode: "auto".to_string(),
            llm_base_url: None,
            llm_model: None,
            llm_api_key: None,
            engine: "auto".to_string(),
            timeout: 30000,
        };

        assert!(build_llm_config(&request).is_none());
    }
}
