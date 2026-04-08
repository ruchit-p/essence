use regex::Regex;
use std::sync::LazyLock;

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
    sources.sort_by(|a, b| b.width.unwrap_or(0).cmp(&a.width.unwrap_or(0)));

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
        Some((multiplier * 600.0) as u32) // Convert to pseudo-width
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
    static RE_IMG_SRCSET: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<img[^>]*srcset="[^"]*"[^>]*>"#).unwrap());
    static RE_SRCSET_ATTR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"srcset="([^"]*)""#).unwrap());
    static RE_SRC_ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"src="[^"]*""#).unwrap());
    static RE_SRCSET_REMOVE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"\s*srcset="[^"]*""#).unwrap());

    let mut result = html.to_string();
    let mut replacements: Vec<(String, String)> = Vec::new();

    for img_match in RE_IMG_SRCSET.find_iter(html) {
        let old_tag = img_match.as_str();

        if let Some(srcset_cap) = RE_SRCSET_ATTR.captures(old_tag) {
            let srcset = &srcset_cap[1];

            if let Some(largest) = parse_srcset_pick_largest(srcset) {
                let mut new_tag = old_tag.to_string();

                if RE_SRC_ATTR.is_match(&new_tag) {
                    new_tag = RE_SRC_ATTR
                        .replace(&new_tag, &format!(r#"src="{}""#, largest))
                        .to_string();
                } else {
                    new_tag = new_tag.replace("<img ", &format!(r#"<img src="{}" "#, largest));
                }

                new_tag = RE_SRCSET_REMOVE.replace(&new_tag, "").to_string();
                replacements.push((old_tag.to_string(), new_tag));
            }
        }
    }

    for (old, new) in replacements {
        result = result.replace(&old, &new);
    }

    result
}

/// Rescue <img> tags from <noscript> blocks before noscript gets stripped.
/// Many sites put the real <img> inside <noscript> while using lazy-loading JS
/// in the main content. This extracts those images so they survive stripping.
pub fn rescue_noscript_images(html: &str) -> String {
    static RE_NOSCRIPT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<noscript[^>]*>(.*?)</noscript>").unwrap());
    static RE_IMG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<img\s[^>]*>").unwrap());

    let mut rescued = Vec::new();
    for cap in RE_NOSCRIPT.captures_iter(html) {
        let inner = &cap[1];
        for img in RE_IMG.find_iter(inner) {
            rescued.push(img.as_str().to_string());
        }
    }

    if rescued.is_empty() {
        return html.to_string();
    }

    // Insert rescued images just before </body> or at the end
    let insertion = rescued.join("\n");
    if let Some(pos) = html.to_lowercase().rfind("</body>") {
        let mut result = html.to_string();
        result.insert_str(pos, &format!("\n{}\n", insertion));
        result
    } else {
        format!("{}\n{}", html, insertion)
    }
}

/// Resolve <picture> elements to simple <img> tags by picking the largest <source>.
pub fn resolve_picture_elements(html: &str) -> String {
    static RE_PICTURE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<picture[^>]*>(.*?)</picture>").unwrap());
    static RE_SOURCE_SRCSET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<source[^>]*srcset\s*=\s*["']([^"']+)["'][^>]*>"#).unwrap()
    });
    static RE_IMG_TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<img\s[^>]*>").unwrap());

    RE_PICTURE
        .replace_all(html, |caps: &regex::Captures| {
            let inner = &caps[1];

            // Try to find the best source from <source srcset="...">
            let mut best_url: Option<String> = None;
            for source_cap in RE_SOURCE_SRCSET.captures_iter(inner) {
                let srcset = &source_cap[1];
                if let Some(url) = parse_srcset_pick_largest(srcset) {
                    best_url = Some(url);
                }
            }

            // If we found a <source>, build an <img> with that URL
            if let Some(url) = best_url {
                // Try to preserve alt from the fallback <img>
                if let Some(img_match) = RE_IMG_TAG.find(inner) {
                    let img_tag = img_match.as_str();
                    let alt_regex = Regex::new(r#"alt\s*=\s*["']([^"']*?)["']"#).unwrap();
                    let alt = alt_regex
                        .captures(img_tag)
                        .map(|c| c[1].to_string())
                        .unwrap_or_default();
                    format!(r#"<img src="{}" alt="{}">"#, url, alt)
                } else {
                    format!(r#"<img src="{}" alt="">"#, url)
                }
            } else if let Some(img_match) = RE_IMG_TAG.find(inner) {
                // No <source> found, just use the fallback <img>
                img_match.as_str().to_string()
            } else {
                String::new()
            }
        })
        .to_string()
}

/// Resolve lazy-loaded images by promoting data-src, data-lazy-src, etc. to src.
pub fn resolve_lazy_images(html: &str) -> String {
    static RE_LAZY_IMG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<img\s[^>]*data-(?:src|lazy-src|original|lazy-load)\s*=\s*["'][^"']+["'][^>]*>"#).unwrap()
    });
    static RE_DATA_SRC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"data-(?:src|lazy-src|original|lazy-load)\s*=\s*["']([^"']+)["']"#).unwrap()
    });
    static RE_HAS_REAL_SRC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)\bsrc\s*=\s*["']([^"']+)["']"#).unwrap());

    RE_LAZY_IMG
        .replace_all(html, |caps: &regex::Captures| {
            let tag = &caps[0];

            // Extract the lazy data-src URL
            if let Some(data_cap) = RE_DATA_SRC.captures(tag) {
                let lazy_url = &data_cap[1];

                // If there's already a real src (not data: URI), keep the tag as-is
                if let Some(src_cap) = RE_HAS_REAL_SRC.captures(tag) {
                    if !src_cap[1].starts_with("data:") {
                        return tag.to_string();
                    }
                }

                // Replace or add src with the lazy URL
                let src_attr = Regex::new(r#"src\s*=\s*["'][^"']*["']"#).unwrap();
                if src_attr.is_match(tag) {
                    src_attr
                        .replace(tag, &format!(r#"src="{}""#, lazy_url))
                        .to_string()
                } else {
                    tag.replace("<img ", &format!(r#"<img src="{}" "#, lazy_url))
                }
            } else {
                tag.to_string()
            }
        })
        .to_string()
}

/// Extract video poster frames as images so they appear in markdown output.
pub fn resolve_video_posters(html: &str) -> String {
    static RE_VIDEO_POSTER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<video[^>]*poster\s*=\s*["']([^"']+)["'][^>]*>.*?</video>"#).unwrap()
    });

    RE_VIDEO_POSTER
        .replace_all(html, |caps: &regex::Captures| {
            let poster_url = &caps[1];
            let original = &caps[0];
            // Keep the original video tag and append a poster image
            format!(
                r#"{}<img src="{}" alt="Video poster">"#,
                original, poster_url
            )
        })
        .to_string()
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
