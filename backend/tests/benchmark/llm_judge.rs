// LLM-as-Judge evaluation using Claude Code headless mode (claude -p).
//
// Performs pairwise comparison of markdown outputs from two scraping engines,
// returning structured verdicts on 5 quality dimensions.
//
// Controlled by LLM_JUDGE=true environment variable (off by default).
// Requires `claude` CLI to be available in PATH.

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Instant;

// MARK: - Data Structures

/// Structured verdict from the LLM judge for a single URL comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmVerdict {
    pub content_relevance: DimensionVerdict,
    pub noise_removal: DimensionVerdict,
    pub readability: DimensionVerdict,
    pub structural_coherence: DimensionVerdict,
    pub information_completeness: DimensionVerdict,
    pub token_efficiency: DimensionVerdict,
    pub overall_winner: String,
    pub overall_reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionVerdict {
    pub winner: String,
    pub reasoning: String,
}

/// Result of an LLM evaluation attempt
pub struct EvalResult {
    pub verdict: Option<LlmVerdict>,
    pub raw_json: serde_json::Value,
    pub elapsed_ms: u128,
    pub error: Option<String>,
}

// MARK: - Configuration

/// Check whether LLM judge is enabled via environment.
/// Default: ON. Set LLM_JUDGE=false to disable for fast heuristic-only runs.
pub fn is_enabled() -> bool {
    std::env::var("LLM_JUDGE")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true)
}

/// Check whether the `claude` CLI is available
pub fn is_claude_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// MARK: - JSON Schema

const VERDICT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "content_relevance": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "noise_removal": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "readability": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "structural_coherence": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "information_completeness": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "token_efficiency": {
      "type": "object",
      "properties": {
        "winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
        "reasoning": { "type": "string" }
      },
      "required": ["winner", "reasoning"]
    },
    "overall_winner": { "type": "string", "enum": ["essence", "firecrawl", "tie"] },
    "overall_reasoning": { "type": "string" }
  },
  "required": ["content_relevance", "noise_removal", "readability", "structural_coherence", "information_completeness", "token_efficiency", "overall_winner", "overall_reasoning"]
}"#;

// MARK: - Evaluation

/// Truncate markdown to a reasonable size for LLM evaluation.
/// Keeps the first N chars plus a tail sample for context.
fn truncate_markdown(md: &str, max_chars: usize) -> String {
    if md.len() <= max_chars {
        return md.to_string();
    }
    let head_size = max_chars * 3 / 4;
    let tail_size = max_chars / 4;
    let head = &md[..head_size];
    let tail = &md[md.len() - tail_size..];
    format!(
        "{}\n\n[... {} chars truncated ...]\n\n{}",
        head,
        md.len() - head_size - tail_size,
        tail
    )
}

/// Build the comparison prompt for the LLM judge
fn build_prompt(url: &str, description: &str, essence_md: &str, firecrawl_md: &str) -> String {
    let e_truncated = truncate_markdown(essence_md, 4000);
    let f_truncated = truncate_markdown(firecrawl_md, 4000);

    format!(
        r#"You are evaluating which scraping engine produces better output for AI agent consumption. The ideal output captures the complete essence of the page in clean, well-structured markdown — maximizing information while minimizing noise and token waste.

URL: {url}
Description: {description}

Evaluate which engine produced better output across these 6 dimensions:

1. **Content Relevance**: Does the output capture the page's main content (article text, documentation, product info)? Or does it include mostly irrelevant elements (navigation, sidebars, unrelated recommendations)?
2. **Noise Removal**: How well are navigation menus, advertisements, footers, cookie banners, tracking pixels, spacer images, and boilerplate removed? Less noise = better.
3. **Readability**: Is the markdown clean, well-formatted, and easy to read? Good heading hierarchy, proper link formatting, no garbled text or escaped HTML artifacts?
4. **Structural Coherence**: Are headings, lists, tables, and code blocks logically structured and properly nested? Are lists preserved as lists, tables as tables?
5. **Information Completeness**: Is all important information from the page preserved? Nothing critical missing — all key text, data, and structural elements are present?
6. **Token Efficiency**: Is the output concise without losing information? No redundant whitespace, duplicate content, tracking pixels, empty links, or unnecessary formatting that wastes tokens? Shorter output with the same information is better.

For each dimension, determine the winner: "essence", "firecrawl", or "tie".

--- ENGINE A (essence) ---
{e_truncated}
--- END ENGINE A ---

--- ENGINE B (firecrawl) ---
{f_truncated}
--- END ENGINE B ---

Be objective and specific in your reasoning. Focus on what matters for an AI agent consuming this content — clean, complete, token-efficient markdown."#
    )
}

/// Extract the result object from claude -p output.
/// Handles both array format (list of message events) and single object format.
fn extract_result_from_claude_output(json: &serde_json::Value) -> serde_json::Value {
    if let Some(arr) = json.as_array() {
        // Array of message events — find the "result" type entry
        for item in arr.iter().rev() {
            if item.get("type").and_then(|t| t.as_str()) == Some("result") {
                return item.clone();
            }
        }
        // Fallback: last item in array
        arr.last().cloned().unwrap_or(serde_json::Value::Null)
    } else {
        json.clone()
    }
}

/// Extract the verdict JSON from a result object's "result" text field.
/// The "result" field may be a JSON string that needs re-parsing.
fn extract_result_text(result_obj: &serde_json::Value) -> serde_json::Value {
    if let Some(result_str) = result_obj.get("result").and_then(|r| r.as_str()) {
        serde_json::from_str(result_str).unwrap_or_else(|_| {
            serde_json::Value::String(result_str.to_string())
        })
    } else if let Some(result_val) = result_obj.get("result") {
        if !result_val.is_null() {
            return result_val.clone();
        }
        result_obj.clone()
    } else {
        result_obj.clone()
    }
}

/// Evaluate a single URL pair using Claude Code headless mode.
/// Returns the structured verdict or an error.
pub fn evaluate(
    url: &str,
    description: &str,
    essence_markdown: &str,
    firecrawl_markdown: &str,
) -> EvalResult {
    let prompt = build_prompt(url, description, essence_markdown, firecrawl_markdown);
    let start = Instant::now();

    let output = Command::new("claude")
        .args([
            "-p",
            &prompt,
            "--output-format",
            "json",
            "--json-schema",
            VERDICT_SCHEMA,
        ])
        .env_remove("CLAUDECODE")
        .output();

    let elapsed_ms = start.elapsed().as_millis();

    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                return EvalResult {
                    verdict: None,
                    raw_json: serde_json::Value::Null,
                    elapsed_ms,
                    error: Some(format!("claude -p failed: {}", stderr.trim())),
                };
            }

            let stdout = String::from_utf8_lossy(&result.stdout);

            // claude -p --output-format json may return either:
            //   1. A JSON array of message events (newer CLI) — find the "result" type entry
            //   2. A single JSON object with result/structured_output fields
            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(json) => {
                    let result_obj = extract_result_from_claude_output(&json);

                    let verdict_json = if let Some(so) = result_obj.get("structured_output") {
                        if !so.is_null() { so.clone() } else { extract_result_text(&result_obj) }
                    } else {
                        extract_result_text(&result_obj)
                    };

                    let verdict: Option<LlmVerdict> =
                        serde_json::from_value(verdict_json.clone()).ok();

                    EvalResult {
                        verdict,
                        raw_json: verdict_json,
                        elapsed_ms,
                        error: None,
                    }
                }
                Err(e) => EvalResult {
                    verdict: None,
                    raw_json: serde_json::Value::Null,
                    elapsed_ms,
                    error: Some(format!("Failed to parse claude output: {}", e)),
                },
            }
        }
        Err(e) => EvalResult {
            verdict: None,
            raw_json: serde_json::Value::Null,
            elapsed_ms,
            error: Some(format!("Failed to invoke claude: {}", e)),
        },
    }
}
