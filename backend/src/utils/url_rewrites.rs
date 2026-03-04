use tracing::debug;
use url::Url;

/// Rewrite URLs to get better content
pub fn rewrite_url(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return url.to_string();
    };

    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path();

    // Google Slides: → PDF export (check first to avoid conflict with Google Docs)
    if host.contains("docs.google.com") && path.contains("/presentation/") && path.contains("/edit") {
        let rewritten = url.replace("/edit", "/export/pdf");
        debug!("Rewrote Google Slides URL: {} → {}", url, rewritten);
        return rewritten;
    }

    // Google Docs: /edit → /export?format=pdf
    if host.contains("docs.google.com") && path.contains("/document/") && path.contains("/edit") {
        let rewritten = url.replace("/edit", "/export?format=pdf");
        debug!("Rewrote Google Docs URL: {} → {}", url, rewritten);
        return rewritten;
    }

    // Google Sheets: → HTML export
    if host.contains("docs.google.com") && path.contains("/spreadsheets/") && path.contains("/edit") {
        // Extract document ID
        if let Some(doc_id) = extract_google_doc_id(path) {
            let rewritten = format!(
                "https://docs.google.com/spreadsheets/d/{}/gviz/tq?tqx=out:html",
                doc_id
            );
            debug!("Rewrote Google Sheets URL: {} → {}", url, rewritten);
            return rewritten;
        }
    }

    // Google Drive: /view → /uc?export=download
    if host.contains("drive.google.com") && path.contains("/file/") && path.contains("/view") {
        // Extract file ID
        if let Some(file_id) = extract_google_drive_id(path) {
            let rewritten = format!(
                "https://drive.google.com/uc?export=download&id={}",
                file_id
            );
            debug!("Rewrote Google Drive URL: {} → {}", url, rewritten);
            return rewritten;
        }
    }

    // No rewrite needed
    url.to_string()
}

/// Extract Google Doc ID from path
/// Path format: /document/d/{ID}/edit or /spreadsheets/d/{ID}/edit
fn extract_google_doc_id(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();

    // Find "d" segment, next one is the ID
    for (i, part) in parts.iter().enumerate() {
        if *part == "d" && i + 1 < parts.len() {
            return Some(parts[i + 1].to_string());
        }
    }

    None
}

/// Extract Google Drive file ID from path
/// Path format: /file/d/{ID}/view
fn extract_google_drive_id(path: &str) -> Option<String> {
    extract_google_doc_id(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_docs_rewrite() {
        let input = "https://docs.google.com/document/d/ABC123/edit";
        let expected = "https://docs.google.com/document/d/ABC123/export?format=pdf";
        assert_eq!(rewrite_url(input), expected);
    }

    #[test]
    fn test_google_sheets_rewrite() {
        let input = "https://docs.google.com/spreadsheets/d/XYZ456/edit";
        let expected = "https://docs.google.com/spreadsheets/d/XYZ456/gviz/tq?tqx=out:html";
        assert_eq!(rewrite_url(input), expected);
    }

    #[test]
    fn test_google_drive_rewrite() {
        let input = "https://drive.google.com/file/d/FILE123/view";
        let expected = "https://drive.google.com/uc?export=download&id=FILE123";
        assert_eq!(rewrite_url(input), expected);
    }

    #[test]
    fn test_google_slides_rewrite() {
        let input = "https://docs.google.com/presentation/d/PRES456/edit";
        let expected = "https://docs.google.com/presentation/d/PRES456/export/pdf";
        assert_eq!(rewrite_url(input), expected);
    }

    #[test]
    fn test_no_rewrite_needed() {
        let input = "https://example.com/page.html";
        assert_eq!(rewrite_url(input), input);
    }

    #[test]
    fn test_already_export_url() {
        let input = "https://docs.google.com/document/d/ABC123/export?format=pdf";
        assert_eq!(rewrite_url(input), input); // Should not double-rewrite
    }
}
