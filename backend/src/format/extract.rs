use crate::error::{Result, ScrapeError};
use scraper::{Html, Selector};
use std::collections::HashMap;
use tracing::debug;

/// Extract structured data from HTML using CSS selector mappings.
///
/// Each entry in `selectors` maps a field name to a CSS selector.
/// The function runs each selector against the HTML and extracts text content.
/// If a `schema` is provided, type coercion is attempted (e.g. "type": "number").
pub fn extract_with_css(
    html: &str,
    selectors: &HashMap<String, String>,
    schema: Option<&serde_json::Value>,
) -> Result<serde_json::Value> {
    let document = Html::parse_document(html);
    let mut result = serde_json::Map::new();

    let schema_props = schema
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object());

    for (field_name, css_selector) in selectors {
        let selector = Selector::parse(css_selector).map_err(|e| {
            ScrapeError::InvalidRequest(format!(
                "Invalid CSS selector for field '{}': {:?}",
                field_name, e
            ))
        })?;

        let field_schema = schema_props.and_then(|props| props.get(field_name.as_str()));
        let field_type = field_schema
            .and_then(|s| s.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("string");

        let value = match field_type {
            "array" => {
                // Collect all matching elements
                let items: Vec<serde_json::Value> = document
                    .select(&selector)
                    .map(|el| {
                        let text = el.text().collect::<String>().trim().to_string();
                        coerce_to_item_type(field_schema, &text)
                    })
                    .collect();
                serde_json::Value::Array(items)
            }
            "number" | "integer" => {
                let text = first_match_text(&document, &selector);
                match text {
                    Some(t) => parse_number(&t),
                    None => serde_json::Value::Null,
                }
            }
            "boolean" => {
                let text = first_match_text(&document, &selector);
                match text {
                    Some(t) => {
                        let lower = t.to_lowercase();
                        serde_json::Value::Bool(lower == "true" || lower == "yes" || lower == "1")
                    }
                    None => serde_json::Value::Null,
                }
            }
            _ => {
                // Default: string
                let text = first_match_text(&document, &selector);
                match text {
                    Some(t) => serde_json::Value::String(t),
                    None => serde_json::Value::Null,
                }
            }
        };

        debug!("CSS extract: {} = {:?}", field_name, value);
        result.insert(field_name.clone(), value);
    }

    Ok(serde_json::Value::Object(result))
}

/// Check how complete a CSS extraction result is (fraction of non-null fields).
pub fn extraction_completeness(result: &serde_json::Value) -> f64 {
    let obj = match result.as_object() {
        Some(o) => o,
        None => return 0.0,
    };
    if obj.is_empty() {
        return 0.0;
    }
    let non_null = obj.values().filter(|v| !v.is_null()).count();
    non_null as f64 / obj.len() as f64
}

fn first_match_text(document: &Html, selector: &Selector) -> Option<String> {
    document.select(selector).next().map(|el| {
        // Try value attribute for input elements
        if let Some(val) = el.value().attr("content").or(el.value().attr("value")) {
            return val.trim().to_string();
        }
        el.text().collect::<String>().trim().to_string()
    })
}

fn parse_number(text: &str) -> serde_json::Value {
    // Strip currency symbols and commas
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if let Ok(n) = cleaned.parse::<f64>() {
        serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    }
}

fn coerce_to_item_type(field_schema: Option<&serde_json::Value>, text: &str) -> serde_json::Value {
    let item_type = field_schema
        .and_then(|s| s.get("items"))
        .and_then(|i| i.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    match item_type {
        "number" | "integer" => parse_number(text),
        "boolean" => {
            let lower = text.to_lowercase();
            serde_json::Value::Bool(lower == "true" || lower == "yes" || lower == "1")
        }
        _ => serde_json::Value::String(text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_css_extract_basic() {
        let html = r#"
            <html><body>
                <h1 class="title">Awesome Product</h1>
                <span class="price">$29.99</span>
                <p class="desc">A really great product for testing.</p>
            </body></html>
        "#;

        let mut selectors = HashMap::new();
        selectors.insert("title".to_string(), "h1.title".to_string());
        selectors.insert("price".to_string(), "span.price".to_string());
        selectors.insert("description".to_string(), "p.desc".to_string());

        let result = extract_with_css(html, &selectors, None).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj["title"], "Awesome Product");
        assert_eq!(obj["price"], "$29.99");
        assert_eq!(obj["description"], "A really great product for testing.");
    }

    #[test]
    fn test_css_extract_with_schema_coercion() {
        let html = r#"
            <html><body>
                <span class="price">$29.99</span>
                <span class="count">42</span>
                <span class="available">true</span>
            </body></html>
        "#;

        let schema = serde_json::json!({
            "properties": {
                "price": {"type": "number"},
                "count": {"type": "integer"},
                "available": {"type": "boolean"}
            }
        });

        let mut selectors = HashMap::new();
        selectors.insert("price".to_string(), "span.price".to_string());
        selectors.insert("count".to_string(), "span.count".to_string());
        selectors.insert("available".to_string(), "span.available".to_string());

        let result = extract_with_css(html, &selectors, Some(&schema)).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj["price"], 29.99);
        assert_eq!(obj["count"], 42.0);
        assert_eq!(obj["available"], true);
    }

    #[test]
    fn test_css_extract_array() {
        let html = r#"
            <html><body>
                <ul>
                    <li class="item">Apple</li>
                    <li class="item">Banana</li>
                    <li class="item">Cherry</li>
                </ul>
            </body></html>
        "#;

        let schema = serde_json::json!({
            "properties": {
                "items": {"type": "array", "items": {"type": "string"}}
            }
        });

        let mut selectors = HashMap::new();
        selectors.insert("items".to_string(), "li.item".to_string());

        let result = extract_with_css(html, &selectors, Some(&schema)).unwrap();
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], "Apple");
        assert_eq!(items[1], "Banana");
        assert_eq!(items[2], "Cherry");
    }

    #[test]
    fn test_css_extract_missing_element() {
        let html = "<html><body><p>Hello</p></body></html>";
        let mut selectors = HashMap::new();
        selectors.insert("missing".to_string(), "h1.nonexistent".to_string());

        let result = extract_with_css(html, &selectors, None).unwrap();
        assert!(result["missing"].is_null());
    }

    #[test]
    fn test_extraction_completeness() {
        let full = serde_json::json!({"a": "val", "b": 42});
        assert_eq!(extraction_completeness(&full), 1.0);

        let partial = serde_json::json!({"a": "val", "b": null});
        assert_eq!(extraction_completeness(&partial), 0.5);

        let empty = serde_json::json!({});
        assert_eq!(extraction_completeness(&empty), 0.0);
    }
}
