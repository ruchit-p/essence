use crate::{
    engines::{detection::RenderingDetector, RawScrapeResult},
    error::Result,
    format::advanced_extraction::AdvancedExtractor,
    types::{Metadata, ScrapeRequest},
};
use scraper::{Html, Selector};

/// Extract metadata from raw scrape result
pub fn extract_metadata(raw: &RawScrapeResult, _request: &ScrapeRequest) -> Result<Metadata> {
    let document = Html::parse_document(&raw.html);

    // Try advanced extraction for article content
    let (word_count, reading_time, excerpt, detected_language) =
        if let Ok(article) = AdvancedExtractor::extract_article(&raw.html, &raw.url) {
            (
                Some(article.word_count),
                Some(article.reading_time),
                article.excerpt,
                article.language,
            )
        } else {
            // Fallback to basic extraction if Readability fails
            let text = document.root_element().text().collect::<String>();
            let word_count = AdvancedExtractor::count_words(&text);
            (
                Some(word_count),
                Some(AdvancedExtractor::estimate_reading_time(word_count)),
                AdvancedExtractor::generate_excerpt(&text),
                AdvancedExtractor::detect_language(&text),
            )
        };

    // Perform JS detection for metadata
    let detection = RenderingDetector::needs_javascript(&raw.html, &raw.url);

    Ok(Metadata {
        title: extract_title(&document),
        description: extract_description(&document),
        language: extract_language(&document).or(detected_language),
        keywords: extract_keywords(&document),
        robots: extract_robots(&document),
        og_title: extract_og_tag(&document, "og:title"),
        og_description: extract_og_tag(&document, "og:description"),
        og_url: extract_og_tag(&document, "og:url"),
        og_image: extract_og_tag(&document, "og:image"),
        url: Some(raw.url.clone()),
        source_url: Some(raw.url.clone()),
        status_code: raw.status_code,
        content_type: raw.content_type.clone(),
        canonical_url: extract_canonical_url(&document),
        word_count,
        reading_time,
        excerpt,
        detected_frameworks: if detection.detected_frameworks.is_empty() {
            None
        } else {
            Some(detection.detected_frameworks)
        },
        detection_reason: Some(detection.reason),
        content_script_ratio: Some(detection.content_script_ratio),
    })
}

/// Extract page title from <title> tag
fn extract_title(document: &Html) -> Option<String> {
    let selector = Selector::parse("title").ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract description from meta tag, with fallbacks to OG, Twitter, and first paragraph
fn extract_description(document: &Html) -> Option<String> {
    extract_meta_content(document, "name", "description")
        .or_else(|| extract_meta_content(document, "property", "description"))
        .or_else(|| extract_meta_content(document, "property", "og:description"))
        .or_else(|| extract_meta_content(document, "name", "twitter:description"))
        .or_else(|| extract_first_paragraph(document))
}

/// Extract a description from the first meaningful text block of the page.
/// Used as a last resort when no meta description is available.
/// Checks p, div, font, td, li elements in priority order.
fn extract_first_paragraph(document: &Html) -> Option<String> {
    // Try multiple selectors in order of semantic priority
    let selectors = ["p", "div", "font", "td", "li"];

    for sel_str in &selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            for el in document.select(&selector) {
                let text = el.text().collect::<String>().trim().to_string();
                // Only use elements with substantial text (> 80 chars)
                // and that look like content (not navigation/UI)
                if text.len() > 80 && !looks_like_navigation(&text) {
                    // Truncate to ~200 chars at word boundary
                    let desc = if text.len() > 200 {
                        match text[..200].rfind(' ') {
                            Some(pos) => format!("{}...", &text[..pos]),
                            None => format!("{}...", &text[..200]),
                        }
                    } else {
                        text
                    };
                    return Some(desc);
                }
            }
        }
    }
    None
}

/// Check if text looks like navigation rather than content
fn looks_like_navigation(text: &str) -> bool {
    let lower = text.to_lowercase();
    // Navigation text is usually short items with specific patterns
    lower.starts_with("skip to")
        || lower.starts_with("menu")
        || lower.starts_with("search")
        || (text.len() < 150 && text.matches('\n').count() > 5) // Many short lines = nav
}

/// Extract language from <html lang="...">
fn extract_language(document: &Html) -> Option<String> {
    let selector = Selector::parse("html").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(|s| s.to_string())
}

/// Extract keywords from meta tag
fn extract_keywords(document: &Html) -> Option<String> {
    extract_meta_content(document, "name", "keywords")
}

/// Extract robots from meta tag
fn extract_robots(document: &Html) -> Option<String> {
    extract_meta_content(document, "name", "robots")
}

/// Extract Open Graph tag
fn extract_og_tag(document: &Html, property: &str) -> Option<String> {
    extract_meta_content(document, "property", property)
}

/// Extract canonical URL from <link rel="canonical">
fn extract_canonical_url(document: &Html) -> Option<String> {
    let selector = Selector::parse("link[rel='canonical']").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| s.to_string())
}

/// Generic meta tag extractor
fn extract_meta_content(document: &Html, attr_name: &str, attr_value: &str) -> Option<String> {
    let selector_str = format!("meta[{}='{}']", attr_name, attr_value);
    let selector = Selector::parse(&selector_str).ok()?;

    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = "<html><head><title>Test Page</title></head></html>";
        let doc = Html::parse_document(html);
        let title = extract_title(&doc);
        assert_eq!(title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_extract_description() {
        let html =
            r#"<html><head><meta name="description" content="Test description"></head></html>"#;
        let doc = Html::parse_document(html);
        let desc = extract_description(&doc);
        assert_eq!(desc, Some("Test description".to_string()));
    }

    #[test]
    fn test_extract_description_og_fallback() {
        // When no standard description exists, should fall back to og:description
        let html = r#"<html><head><meta property="og:description" content="OG desc"></head></html>"#;
        let doc = Html::parse_document(html);
        let desc = extract_description(&doc);
        assert_eq!(desc, Some("OG desc".to_string()));
    }

    #[test]
    fn test_extract_description_twitter_fallback() {
        // When no standard or OG description exists, should fall back to twitter:description
        let html =
            r#"<html><head><meta name="twitter:description" content="Twitter desc"></head></html>"#;
        let doc = Html::parse_document(html);
        let desc = extract_description(&doc);
        assert_eq!(desc, Some("Twitter desc".to_string()));
    }

    #[test]
    fn test_extract_description_prefers_standard() {
        // Standard description should take priority over OG
        let html = r#"<html><head>
            <meta name="description" content="Standard desc">
            <meta property="og:description" content="OG desc">
        </head></html>"#;
        let doc = Html::parse_document(html);
        let desc = extract_description(&doc);
        assert_eq!(desc, Some("Standard desc".to_string()));
    }

    #[test]
    fn test_extract_og_tags() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="OG Title">
                    <meta property="og:description" content="OG Description">
                    <meta property="og:image" content="https://example.com/image.jpg">
                </head>
            </html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(
            extract_og_tag(&doc, "og:title"),
            Some("OG Title".to_string())
        );
        assert_eq!(
            extract_og_tag(&doc, "og:description"),
            Some("OG Description".to_string())
        );
        assert_eq!(
            extract_og_tag(&doc, "og:image"),
            Some("https://example.com/image.jpg".to_string())
        );
    }

    #[test]
    fn test_extract_canonical_url() {
        let html = r#"<html><head><link rel="canonical" href="https://example.com/canonical"></head></html>"#;
        let doc = Html::parse_document(html);
        let canonical = extract_canonical_url(&doc);
        assert_eq!(canonical, Some("https://example.com/canonical".to_string()));
    }

    #[test]
    fn test_extract_language() {
        let html = r#"<html lang="en-US"><head></head></html>"#;
        let doc = Html::parse_document(html);
        let lang = extract_language(&doc);
        assert_eq!(lang, Some("en-US".to_string()));
    }
}
