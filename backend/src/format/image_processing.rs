use regex::Regex;

/// Parse a srcset attribute and return the largest image
/// Example: "small.jpg 300w, medium.jpg 600w, large.jpg 1200w" -> "large.jpg"
pub fn parse_srcset_pick_largest(srcset: &str) -> Option<String> {
    if srcset.trim().is_empty() {
        return None;
    }

    let mut sources: Vec<ImageSource> = Vec::new();

    for entry in srcset.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        // Parse "url 300w" or "url 2x"
        let parts: Vec<&str> = entry.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let url = parts[0].to_string();
        let width = if parts.len() > 1 {
            parse_width_descriptor(parts[1])
        } else {
            None
        };

        sources.push(ImageSource { url, width });
    }

    // Sort by width (descending), then return first
    sources.sort_by(|a, b| {
        b.width.unwrap_or(0).cmp(&a.width.unwrap_or(0))
    });

    sources.first().map(|s| s.url.clone())
}

/// Parse width descriptor like "300w" or "2x"
fn parse_width_descriptor(desc: &str) -> Option<u32> {
    if desc.ends_with('w') {
        // Width in pixels: "300w"
        desc.trim_end_matches('w').parse().ok()
    } else if desc.ends_with('x') {
        // Pixel density: "2x" -> treat as width multiplier
        // Use 600px as base (typical image container width)
        // So 1x=600, 2x=1200, 3x=1800
        let multiplier = desc.trim_end_matches('x').parse::<f32>().ok()?;
        Some((multiplier * 600.0) as u32)  // Convert to pseudo-width
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct ImageSource {
    url: String,
    width: Option<u32>,
}

/// Resolve all srcset attributes in HTML to use largest image
pub fn resolve_srcsets(html: &str) -> String {
    let mut result = html.to_string();

    // Find all img tags with srcset using regex to get the actual tag string
    let img_tag_regex = Regex::new(r#"<img[^>]*srcset="[^"]*"[^>]*>"#).unwrap();

    // Collect all matches first to avoid borrow issues
    let mut replacements: Vec<(String, String)> = Vec::new();

    for img_match in img_tag_regex.find_iter(html) {
        let old_tag = img_match.as_str();

        // Extract srcset attribute value
        let srcset_regex = Regex::new(r#"srcset="([^"]*)""#).unwrap();
        if let Some(srcset_cap) = srcset_regex.captures(old_tag) {
            let srcset = &srcset_cap[1];

            if let Some(largest) = parse_srcset_pick_largest(srcset) {
                // Build new tag
                let mut new_tag = old_tag.to_string();

                // Replace or add src attribute
                let src_regex = Regex::new(r#"src="[^"]*""#).unwrap();
                if src_regex.is_match(&new_tag) {
                    // Replace existing src
                    new_tag = src_regex.replace(&new_tag, &format!(r#"src="{}""#, largest)).to_string();
                } else {
                    // Add new src attribute after <img
                    new_tag = new_tag.replace("<img ", &format!(r#"<img src="{}" "#, largest));
                }

                // Remove srcset attribute
                let srcset_remove_regex = Regex::new(r#"\s*srcset="[^"]*""#).unwrap();
                new_tag = srcset_remove_regex.replace(&new_tag, "").to_string();

                replacements.push((old_tag.to_string(), new_tag));
            }
        }
    }

    // Apply all replacements
    for (old, new) in replacements {
        result = result.replace(&old, &new);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srcset_width_descriptors() {
        let srcset = "small.jpg 300w, medium.jpg 600w, large.jpg 1200w";
        let largest = parse_srcset_pick_largest(srcset).unwrap();
        assert_eq!(largest, "large.jpg");
    }

    #[test]
    fn test_parse_srcset_density_descriptors() {
        let srcset = "small.jpg 1x, large.jpg 2x";
        let largest = parse_srcset_pick_largest(srcset).unwrap();
        assert_eq!(largest, "large.jpg");
    }

    #[test]
    fn test_parse_srcset_mixed() {
        let srcset = "small.jpg 300w, medium.jpg 2x, large.jpg 1600w";
        let largest = parse_srcset_pick_largest(srcset).unwrap();
        assert_eq!(largest, "large.jpg");
    }

    #[test]
    fn test_parse_srcset_single() {
        let srcset = "image.jpg 500w";
        let largest = parse_srcset_pick_largest(srcset).unwrap();
        assert_eq!(largest, "image.jpg");
    }

    #[test]
    fn test_parse_srcset_no_descriptor() {
        let srcset = "image.jpg";
        let largest = parse_srcset_pick_largest(srcset).unwrap();
        assert_eq!(largest, "image.jpg");
    }

    #[test]
    fn test_resolve_srcsets() {
        let html = r#"<img srcset="small.jpg 300w, large.jpg 1200w" src="small.jpg" alt="Test">"#;
        let result = resolve_srcsets(html);
        assert!(result.contains("src=\"large.jpg\""));
        assert!(!result.contains("srcset="));
    }

    #[test]
    fn test_resolve_srcsets_no_existing_src() {
        let html = r#"<img srcset="small.jpg 300w, large.jpg 1200w" alt="Test">"#;
        let result = resolve_srcsets(html);
        assert!(result.contains("src=\"large.jpg\""));
        assert!(!result.contains("srcset="));
    }

    #[test]
    fn test_resolve_srcsets_multiple_images() {
        let html = r#"
            <img srcset="img1-small.jpg 300w, img1-large.jpg 1200w" alt="First">
            <img srcset="img2-small.jpg 400w, img2-large.jpg 1600w" alt="Second">
        "#;
        let result = resolve_srcsets(html);
        assert!(result.contains("src=\"img1-large.jpg\""));
        assert!(result.contains("src=\"img2-large.jpg\""));
        assert!(!result.contains("srcset="));
    }

    #[test]
    fn test_resolve_srcsets_preserves_other_attributes() {
        let html = r#"<img width="100" height="100" srcset="small.jpg 300w, large.jpg 1200w" alt="Logo" class="image">"#;
        let result = resolve_srcsets(html);
        assert!(result.contains("width=\"100\""));
        assert!(result.contains("height=\"100\""));
        assert!(result.contains("alt=\"Logo\""));
        assert!(result.contains("class=\"image\""));
        assert!(result.contains("src=\"large.jpg\""));
        assert!(!result.contains("srcset="));
    }

    #[test]
    fn test_resolve_srcsets_no_srcset_unchanged() {
        let html = r#"<img src="regular.jpg" alt="Normal">"#;
        let result = resolve_srcsets(html);
        assert_eq!(result, html);
    }

    #[test]
    fn test_resolve_srcsets_retina_display() {
        let html = r#"<img srcset="image.jpg 1x, image@2x.jpg 2x, image@3x.jpg 3x" alt="Retina">"#;
        let result = resolve_srcsets(html);
        assert!(result.contains("src=\"image@3x.jpg\""));
        assert!(!result.contains("srcset="));
    }

    #[test]
    fn test_parse_width_descriptor_pixels() {
        assert_eq!(parse_width_descriptor("800w"), Some(800));
        assert_eq!(parse_width_descriptor("1200w"), Some(1200));
    }

    #[test]
    fn test_parse_width_descriptor_retina() {
        assert_eq!(parse_width_descriptor("1x"), Some(600));
        assert_eq!(parse_width_descriptor("2x"), Some(1200));
        assert_eq!(parse_width_descriptor("3x"), Some(1800));
    }

    #[test]
    fn test_parse_width_descriptor_invalid() {
        assert_eq!(parse_width_descriptor("invalid"), None);
        assert_eq!(parse_width_descriptor(""), None);
    }

    #[test]
    fn test_parse_srcset_empty() {
        let srcset = "";
        assert_eq!(parse_srcset_pick_largest(srcset), None);
    }

    #[test]
    fn test_parse_srcset_whitespace_only() {
        let srcset = "   ";
        assert_eq!(parse_srcset_pick_largest(srcset), None);
    }
}
