pub mod advanced_extraction;
pub mod image_processing;
pub mod markdown;
pub mod metadata;

use crate::{
    engines::{validate_scrape_quality, RawScrapeResult},
    error::{Result, ScrapeError},
    types::{Document, ScrapeRequest},
};
use tracing::{debug, warn};

/// Process raw scrape result into formatted document
pub async fn process_scrape_result(
    raw: RawScrapeResult,
    request: &ScrapeRequest,
) -> Result<Document> {
    // Extract metadata first
    let metadata = metadata::extract_metadata(&raw, request)?;

    let mut doc = Document {
        title: metadata.title.clone(),
        description: metadata.description.clone(),
        url: Some(raw.url.clone()),
        markdown: None,
        html: None,
        raw_html: None,
        links: None,
        images: None,
        screenshot: None,
        metadata,
    };

    // Process each requested format
    for format in &request.formats {
        match format.as_str() {
            "markdown" => {
                let markdown_content = markdown::html_to_markdown(
                    &raw.html,
                    &raw.url,
                    request.only_main_content,
                )?;

                // Validate content quality
                if let Err(e) = validate_scrape_quality(&raw, &markdown_content) {
                    // Log quality issues but allow auth/rate-limit errors to suggest retry
                    match raw.status_code {
                        401 | 403 => {
                            warn!(
                                "Authentication or permission error ({}): {}. Consider using browser engine or different authentication.",
                                raw.status_code, e
                            );
                            return Err(e);
                        }
                        429 => {
                            warn!(
                                "Rate limit hit (429): {}. Consider adding wait_for or retry with backoff.",
                                e
                            );
                            return Err(e);
                        }
                        _ => {
                            debug!("Content quality validation failed: {}", e);
                            return Err(e);
                        }
                    }
                }

                doc.markdown = Some(markdown_content);
            }
            "html" => {
                doc.html = Some(if request.only_main_content {
                    markdown::extract_main_content_html(&raw.html)?
                } else {
                    raw.html.clone()
                });
            }
            "rawHtml" => {
                doc.raw_html = Some(raw.html.clone());
            }
            "links" => {
                doc.links = Some(extract_links(&raw.html, &raw.url)?);
            }
            "images" => {
                doc.images = Some(extract_images(&raw.html, &raw.url)?);
            }
            unsupported => {
                return Err(ScrapeError::UnsupportedFormat(format!(
                    "Format '{}' is not supported in HTTP-only mode. Supported formats: markdown, html, rawHtml, links, images",
                    unsupported
                )));
            }
        }
    }

    Ok(doc)
}

/// Extract all links from HTML
fn extract_links(html: &str, base_url: &str) -> Result<Vec<String>> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href]")
        .map_err(|e| ScrapeError::ParseError(format!("Invalid selector: {:?}", e)))?;

    let base = url::Url::parse(base_url)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

    let mut links = Vec::new();
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            // Resolve relative URLs
            if let Ok(absolute) = base.join(href) {
                links.push(absolute.to_string());
            }
        }
    }

    Ok(links)
}

/// Extract all image URLs from HTML
fn extract_images(html: &str, base_url: &str) -> Result<Vec<String>> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let selector = Selector::parse("img[src]")
        .map_err(|e| ScrapeError::ParseError(format!("Invalid selector: {:?}", e)))?;

    let base = url::Url::parse(base_url)
        .map_err(|e| ScrapeError::InvalidUrl(format!("Invalid base URL: {}", e)))?;

    let mut images = Vec::new();
    for element in document.select(&selector) {
        if let Some(src) = element.value().attr("src") {
            // Skip base64 images if requested
            if src.starts_with("data:") {
                continue;
            }

            // Resolve relative URLs
            if let Ok(absolute) = base.join(src) {
                images.push(absolute.to_string());
            }
        }
    }

    Ok(images)
}
