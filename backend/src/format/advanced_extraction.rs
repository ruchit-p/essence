use crate::{error::Result, error::ScrapeError};
use readability::extractor::extract;
use scraper::{Html, Selector};
use std::collections::HashMap;
use whatlang::{detect, Lang};

/// Advanced content extractor with article extraction and language detection
pub struct AdvancedExtractor;

/// Extracted article content
#[derive(Debug, Clone)]
pub struct ArticleContent {
    /// Article title
    pub title: Option<String>,
    /// Article text content
    pub text: String,
    /// Article HTML
    pub html: String,
    /// Extracted excerpt
    pub excerpt: Option<String>,
    /// Detected language
    pub language: Option<String>,
    /// Word count
    pub word_count: usize,
    /// Estimated reading time in minutes
    pub reading_time: usize,
}

/// Table data structure
#[derive(Debug, Clone, serde::Serialize)]
pub struct TableData {
    /// Table headers
    pub headers: Vec<String>,
    /// Table rows as maps of header -> value
    pub rows: Vec<HashMap<String, String>>,
}

impl AdvancedExtractor {
    /// Extract article content using Mozilla's Readability algorithm
    pub fn extract_article(html: &str, url: &str) -> Result<ArticleContent> {
        // Use readability to extract main article content
        let article = extract(
            &mut html.as_bytes(),
            &url::Url::parse(url).map_err(|e| {
                ScrapeError::ParseError(format!("Invalid URL for readability: {}", e))
            })?,
        )
        .map_err(|e| ScrapeError::ParseError(format!("Readability extraction failed: {}", e)))?;

        let text = article.text.trim().to_string();
        let word_count = Self::count_words(&text);
        let reading_time = Self::estimate_reading_time(word_count);
        let excerpt = Self::generate_excerpt(&text);
        let language = Self::detect_language(&text);

        Ok(ArticleContent {
            title: Some(article.title),
            text,
            html: article.content,
            excerpt,
            language,
            word_count,
            reading_time,
        })
    }

    /// Generate a smart excerpt with sentence boundary detection
    pub fn generate_excerpt(text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        const MAX_EXCERPT_LENGTH: usize = 200;
        let text = text.trim();

        // If text is shorter than max length, return it as is
        if text.len() <= MAX_EXCERPT_LENGTH {
            return Some(text.to_string());
        }

        // Find the last sentence boundary before MAX_EXCERPT_LENGTH
        let excerpt_candidate = &text[..MAX_EXCERPT_LENGTH];

        // Look for sentence endings: . ! ?
        let sentence_endings = [". ", "! ", "? "];
        let mut last_sentence_end = 0;

        for ending in &sentence_endings {
            if let Some(pos) = excerpt_candidate.rfind(ending) {
                last_sentence_end = last_sentence_end.max(pos + ending.len());
            }
        }

        // If we found a sentence boundary, use it
        if last_sentence_end > 0 {
            return Some(text[..last_sentence_end].trim().to_string());
        }

        // Otherwise, find the last word boundary
        if let Some(last_space) = excerpt_candidate.rfind(' ') {
            return Some(format!("{}...", text[..last_space].trim()));
        }

        // Fallback: just truncate with ellipsis
        Some(format!(
            "{}...",
            &text[..MAX_EXCERPT_LENGTH.min(text.len())]
        ))
    }

    /// Detect language using whatlang
    pub fn detect_language(text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        detect(text).map(|info| {
            match info.lang() {
                Lang::Eng => "en",
                Lang::Spa => "es",
                Lang::Fra => "fr",
                Lang::Deu => "de",
                Lang::Ita => "it",
                Lang::Por => "pt",
                Lang::Rus => "ru",
                Lang::Jpn => "ja",
                Lang::Kor => "ko",
                Lang::Cmn => "zh",
                Lang::Ara => "ar",
                Lang::Hin => "hi",
                Lang::Tur => "tr",
                Lang::Nld => "nl",
                Lang::Pol => "pl",
                Lang::Swe => "sv",
                Lang::Dan => "da",
                Lang::Fin => "fi",
                Lang::Ces => "cs",
                Lang::Ron => "ro",
                Lang::Ukr => "uk",
                Lang::Ell => "el",
                Lang::Hun => "hu",
                Lang::Heb => "he",
                Lang::Tha => "th",
                Lang::Vie => "vi",
                _ => "unknown",
            }
            .to_string()
        })
    }

    /// Extract tables as structured JSON with header mapping
    pub fn extract_tables_as_json(html: &str) -> Result<Vec<TableData>> {
        let document = Html::parse_document(html);
        let table_selector = Selector::parse("table")
            .map_err(|e| ScrapeError::ParseError(format!("Invalid table selector: {:?}", e)))?;

        let mut tables = Vec::new();

        for table in document.select(&table_selector) {
            // Extract headers
            let header_selector = Selector::parse("thead th, thead td").map_err(|e| {
                ScrapeError::ParseError(format!("Invalid header selector: {:?}", e))
            })?;

            let headers: Vec<String> = table
                .select(&header_selector)
                .map(|th| th.text().collect::<String>().trim().to_string())
                .filter(|h| !h.is_empty())
                .collect();

            // If no headers in thead, try first tr
            let headers = if headers.is_empty() {
                let first_row_selector = Selector::parse("tr:first-child th, tr:first-child td")
                    .map_err(|e| {
                        ScrapeError::ParseError(format!("Invalid first row selector: {:?}", e))
                    })?;

                table
                    .select(&first_row_selector)
                    .map(|td| td.text().collect::<String>().trim().to_string())
                    .filter(|h| !h.is_empty())
                    .collect()
            } else {
                headers
            };

            // If still no headers, generate generic ones
            let headers = if headers.is_empty() {
                vec!["Column 1".to_string()]
            } else {
                headers
            };

            // Extract rows - prefer tbody tr, but fall back to all tr if no tbody
            let has_thead = table.select(&Selector::parse("thead").unwrap()).count() > 0;
            let has_tbody = table.select(&Selector::parse("tbody").unwrap()).count() > 0;

            let row_selector = if has_tbody {
                Selector::parse("tbody tr")
            } else {
                Selector::parse("tr")
            }
            .map_err(|e| ScrapeError::ParseError(format!("Invalid row selector: {:?}", e)))?;

            let cell_selector = Selector::parse("td, th")
                .map_err(|e| ScrapeError::ParseError(format!("Invalid cell selector: {:?}", e)))?;

            let mut rows = Vec::new();

            for (i, row) in table.select(&row_selector).enumerate() {
                // Skip the first row if it was used as headers (only when no thead/tbody)
                if i == 0 && !has_thead && !has_tbody {
                    continue;
                }

                let cells: Vec<String> = row
                    .select(&cell_selector)
                    .map(|td| td.text().collect::<String>().trim().to_string())
                    .collect();

                if !cells.is_empty() {
                    let mut row_map = HashMap::new();
                    for (j, cell) in cells.iter().enumerate() {
                        let header = headers
                            .get(j)
                            .cloned()
                            .unwrap_or_else(|| format!("Column {}", j + 1));
                        row_map.insert(header, cell.clone());
                    }
                    rows.push(row_map);
                }
            }

            if !rows.is_empty() {
                tables.push(TableData { headers, rows });
            }
        }

        Ok(tables)
    }

    /// Count words in text
    pub fn count_words(text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Estimate reading time in minutes (assuming 200 words per minute)
    pub fn estimate_reading_time(word_count: usize) -> usize {
        const WORDS_PER_MINUTE: usize = 200;
        word_count.div_ceil(WORDS_PER_MINUTE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_excerpt_short() {
        let text = "This is a short text.";
        let excerpt = AdvancedExtractor::generate_excerpt(text);
        assert_eq!(excerpt, Some(text.to_string()));
    }

    #[test]
    fn test_generate_excerpt_with_sentence() {
        let text = "This is the first sentence. This is the second sentence. This is a very long third sentence that will definitely exceed the maximum excerpt length and should be cut off.";
        let excerpt = AdvancedExtractor::generate_excerpt(text).unwrap();
        assert!(excerpt.contains("first sentence"));
        assert!(excerpt.len() <= 210); // Allow for sentence boundary
                                       // The excerpt should end at a sentence boundary before "cut off"
        if excerpt.len() < text.len() {
            // If truncated, should not include the last part
            assert!(excerpt.ends_with('.') || excerpt.ends_with("..."));
        }
    }

    #[test]
    fn test_detect_language_english() {
        let text = "This is an English text. It contains several sentences to help with language detection.";
        let lang = AdvancedExtractor::detect_language(text);
        assert_eq!(lang, Some("en".to_string()));
    }

    #[test]
    fn test_detect_language_spanish() {
        let text = "Este es un texto en español. Contiene varias oraciones para ayudar con la detección del idioma.";
        let lang = AdvancedExtractor::detect_language(text);
        assert_eq!(lang, Some("es".to_string()));
    }

    #[test]
    fn test_word_count() {
        let text = "This is a test with five words";
        assert_eq!(AdvancedExtractor::count_words(text), 7);
    }

    #[test]
    fn test_reading_time() {
        assert_eq!(AdvancedExtractor::estimate_reading_time(200), 1);
        assert_eq!(AdvancedExtractor::estimate_reading_time(400), 2);
        assert_eq!(AdvancedExtractor::estimate_reading_time(250), 2);
    }

    #[test]
    fn test_extract_tables() {
        let html = r#"
            <table>
                <thead>
                    <tr><th>Name</th><th>Age</th></tr>
                </thead>
                <tbody>
                    <tr><td>Alice</td><td>30</td></tr>
                    <tr><td>Bob</td><td>25</td></tr>
                </tbody>
            </table>
        "#;

        let tables = AdvancedExtractor::extract_tables_as_json(html).unwrap();
        assert_eq!(tables.len(), 1);

        // Debug: print headers to see what we got
        eprintln!("Headers: {:?}", tables[0].headers);

        // The scraper might include whitespace as a third element, so let's filter
        let non_empty_headers: Vec<_> =
            tables[0].headers.iter().filter(|h| !h.is_empty()).collect();

        assert!(non_empty_headers.len() >= 2);
        assert!(tables[0].headers.contains(&"Name".to_string()));
        assert!(tables[0].headers.contains(&"Age".to_string()));
        assert_eq!(tables[0].rows.len(), 2);
        assert_eq!(tables[0].rows[0].get("Name"), Some(&"Alice".to_string()));
        assert_eq!(tables[0].rows[0].get("Age"), Some(&"30".to_string()));
    }
}
