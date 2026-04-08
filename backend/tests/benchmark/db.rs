// SQLite persistence for competitive benchmark results.
// Stores every benchmark run with per-URL metrics for both engines,
// enabling historical comparison and trend analysis.

use rusqlite::{params, Connection, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

// MARK: - Data Structures

/// Summary of a single benchmark run (read back from DB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: i64,
    pub timestamp: String,
    pub git_commit: String,
    pub total_urls: i64,
    pub essence_wins: i64,
    pub firecrawl_wins: i64,
    pub ties: i64,
    pub essence_win_rate: f64,
    pub essence_success_rate: f64,
    pub firecrawl_success_rate: f64,
    pub notes: String,
    // Two-leaderboard fields
    pub quality_win_rate: f64,
    pub speed_win_rate: f64,
}

/// Per-URL result row (read back from DB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlResultRow {
    pub url: String,
    pub category: String,
    pub description: String,

    pub e_success: bool,
    pub e_word_count: i64,
    pub e_markdown_length: i64,
    pub e_heading_count: i64,
    pub e_link_count: i64,
    pub e_image_count: i64,
    pub e_code_block_count: i64,
    pub e_html_artifact_count: i64,
    pub e_content_density: f64,
    pub e_response_time_ms: i64,
    pub e_has_title: bool,
    pub e_has_description: bool,

    pub f_success: bool,
    pub f_word_count: i64,
    pub f_markdown_length: i64,
    pub f_heading_count: i64,
    pub f_link_count: i64,
    pub f_image_count: i64,
    pub f_code_block_count: i64,
    pub f_html_artifact_count: i64,
    pub f_content_density: f64,
    pub f_response_time_ms: i64,
    pub f_has_title: bool,
    pub f_has_description: bool,

    pub overall_winner: String,
    pub essence_advantage: f64,
    pub quality_winner: String,
    pub speed_winner: String,

    pub w_word_count: String,
    pub w_heading_preservation: String,
    pub w_link_preservation: String,
    pub w_image_preservation: String,
    pub w_code_block_preservation: String,
    pub w_markdown_cleanliness: String,
    pub w_content_density: String,
    pub w_metadata_extraction: String,
    pub w_speed: String,
}

/// LLM verdict row (read back from DB)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmVerdictRow {
    pub url: String,
    pub content_relevance_winner: String,
    pub content_relevance_reasoning: String,
    pub noise_removal_winner: String,
    pub noise_removal_reasoning: String,
    pub readability_winner: String,
    pub readability_reasoning: String,
    pub structural_coherence_winner: String,
    pub structural_coherence_reasoning: String,
    pub information_completeness_winner: String,
    pub information_completeness_reasoning: String,
    pub token_efficiency_winner: String,
    pub token_efficiency_reasoning: String,
    pub overall_llm_winner: String,
    pub overall_llm_reasoning: String,
    pub model_used: String,
}

/// Average response time per engine per run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSpeedTrend {
    pub run_id: i64,
    pub avg_essence_ms: f64,
    pub avg_firecrawl_ms: f64,
}

/// Average content metrics per engine per run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetricTrend {
    pub run_id: i64,
    pub avg_e_word_count: f64,
    pub avg_f_word_count: f64,
    pub avg_e_headings: f64,
    pub avg_f_headings: f64,
    pub avg_e_artifacts: f64,
    pub avg_f_artifacts: f64,
    pub essence_success_rate: f64,
    pub firecrawl_success_rate: f64,
}

/// All data needed to render the dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub runs: Vec<RunSummary>,
    pub latest_url_results: Vec<UrlResultRow>,
    pub latest_llm_verdicts: Vec<LlmVerdictRow>,
    /// Per-dimension win rates over time: dimension -> [(run_id, rate)]
    pub dimension_trends: std::collections::HashMap<String, Vec<(i64, f64)>>,
    /// Per-category win rates for the latest run
    pub category_win_rates: std::collections::HashMap<String, f64>,
    /// Average response time per run
    pub speed_trends: Vec<RunSpeedTrend>,
    /// Average content metrics per run
    pub metric_trends: Vec<RunMetricTrend>,
    /// Per-category win rates over time: category -> [(run_id, rate)]
    pub category_trends: std::collections::HashMap<String, Vec<(i64, f64)>>,
    /// Quality (LLM) win rate over time: [(run_id, rate)]
    pub quality_trend: Vec<(i64, f64)>,
}

// MARK: - Database Operations

/// Open (or create) the benchmark SQLite database and initialize schema
pub fn open_db(db_path: &Path) -> SqliteResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    create_tables(&conn)?;
    Ok(conn)
}

fn create_tables(conn: &Connection) -> SqliteResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS benchmark_runs (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp             TEXT    NOT NULL,
            git_commit            TEXT    NOT NULL DEFAULT '',
            total_urls            INTEGER NOT NULL,
            essence_wins          INTEGER NOT NULL,
            firecrawl_wins        INTEGER NOT NULL,
            ties                  INTEGER NOT NULL,
            essence_win_rate      REAL    NOT NULL,
            essence_success_rate  REAL    NOT NULL,
            firecrawl_success_rate REAL   NOT NULL,
            notes                 TEXT    NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS url_results (
            id                        INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id                    INTEGER NOT NULL REFERENCES benchmark_runs(id),
            url                       TEXT    NOT NULL,
            category                  TEXT    NOT NULL,
            description               TEXT    NOT NULL DEFAULT '',

            e_success                 INTEGER NOT NULL,
            e_word_count              INTEGER NOT NULL,
            e_markdown_length         INTEGER NOT NULL,
            e_heading_count           INTEGER NOT NULL,
            e_link_count              INTEGER NOT NULL,
            e_image_count             INTEGER NOT NULL,
            e_code_block_count        INTEGER NOT NULL,
            e_table_count             INTEGER NOT NULL DEFAULT 0,
            e_html_artifact_count     INTEGER NOT NULL,
            e_empty_link_count        INTEGER NOT NULL DEFAULT 0,
            e_base64_count            INTEGER NOT NULL DEFAULT 0,
            e_content_density         REAL    NOT NULL,
            e_has_title               INTEGER NOT NULL,
            e_has_description         INTEGER NOT NULL,
            e_response_time_ms        INTEGER NOT NULL,
            e_content_hash            TEXT    NOT NULL DEFAULT '',

            f_success                 INTEGER NOT NULL,
            f_word_count              INTEGER NOT NULL,
            f_markdown_length         INTEGER NOT NULL,
            f_heading_count           INTEGER NOT NULL,
            f_link_count              INTEGER NOT NULL,
            f_image_count             INTEGER NOT NULL,
            f_code_block_count        INTEGER NOT NULL,
            f_table_count             INTEGER NOT NULL DEFAULT 0,
            f_html_artifact_count     INTEGER NOT NULL,
            f_empty_link_count        INTEGER NOT NULL DEFAULT 0,
            f_base64_count            INTEGER NOT NULL DEFAULT 0,
            f_content_density         REAL    NOT NULL,
            f_has_title               INTEGER NOT NULL,
            f_has_description         INTEGER NOT NULL,
            f_response_time_ms        INTEGER NOT NULL,
            f_content_hash            TEXT    NOT NULL DEFAULT '',

            overall_winner            TEXT    NOT NULL,
            essence_advantage         REAL    NOT NULL,

            w_word_count              TEXT    NOT NULL,
            w_heading_preservation    TEXT    NOT NULL,
            w_link_preservation       TEXT    NOT NULL,
            w_image_preservation      TEXT    NOT NULL,
            w_code_block_preservation TEXT    NOT NULL,
            w_markdown_cleanliness    TEXT    NOT NULL,
            w_content_density         TEXT    NOT NULL,
            w_metadata_extraction     TEXT    NOT NULL,
            w_speed                   TEXT    NOT NULL
        );

        CREATE TABLE IF NOT EXISTS llm_verdicts (
            id                                INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id                            INTEGER NOT NULL REFERENCES benchmark_runs(id),
            url                               TEXT    NOT NULL,
            content_relevance_winner           TEXT    NOT NULL,
            content_relevance_reasoning        TEXT    NOT NULL DEFAULT '',
            noise_removal_winner               TEXT    NOT NULL,
            noise_removal_reasoning            TEXT    NOT NULL DEFAULT '',
            readability_winner                 TEXT    NOT NULL,
            readability_reasoning              TEXT    NOT NULL DEFAULT '',
            structural_coherence_winner        TEXT    NOT NULL,
            structural_coherence_reasoning     TEXT    NOT NULL DEFAULT '',
            information_completeness_winner    TEXT    NOT NULL,
            information_completeness_reasoning TEXT    NOT NULL DEFAULT '',
            overall_llm_winner                 TEXT    NOT NULL,
            overall_llm_reasoning              TEXT    NOT NULL DEFAULT '',
            model_used                         TEXT    NOT NULL DEFAULT '',
            evaluation_time_ms                 INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_url_results_run ON url_results(run_id);
        CREATE INDEX IF NOT EXISTS idx_llm_verdicts_run ON llm_verdicts(run_id);
        ",
    )?;

    // Backward-compatible schema migrations for new columns
    let migrations = [
        "ALTER TABLE benchmark_runs ADD COLUMN quality_win_rate REAL NOT NULL DEFAULT 0.0",
        "ALTER TABLE benchmark_runs ADD COLUMN speed_win_rate REAL NOT NULL DEFAULT 0.0",
        "ALTER TABLE url_results ADD COLUMN quality_winner TEXT NOT NULL DEFAULT 'pending'",
        "ALTER TABLE url_results ADD COLUMN speed_winner TEXT NOT NULL DEFAULT 'tie'",
        "ALTER TABLE llm_verdicts ADD COLUMN token_efficiency_winner TEXT NOT NULL DEFAULT 'tie'",
        "ALTER TABLE llm_verdicts ADD COLUMN token_efficiency_reasoning TEXT NOT NULL DEFAULT ''",
    ];
    for sql in &migrations {
        // Ignore "duplicate column" errors for idempotency
        let _ = conn.execute(sql, []);
    }

    Ok(())
}

/// Get the current git commit short hash
pub fn git_commit_hash() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Insert a new benchmark run and return its ID
pub fn insert_run(
    conn: &Connection,
    timestamp: &str,
    total_urls: usize,
    essence_wins: usize,
    firecrawl_wins: usize,
    ties: usize,
    essence_win_rate: f64,
    essence_success_rate: f64,
    firecrawl_success_rate: f64,
    quality_win_rate: f64,
    speed_win_rate: f64,
) -> SqliteResult<i64> {
    let git = git_commit_hash();
    conn.execute(
        "INSERT INTO benchmark_runs
            (timestamp, git_commit, total_urls, essence_wins, firecrawl_wins,
             ties, essence_win_rate, essence_success_rate, firecrawl_success_rate,
             quality_win_rate, speed_win_rate)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            timestamp,
            git,
            total_urls as i64,
            essence_wins as i64,
            firecrawl_wins as i64,
            ties as i64,
            essence_win_rate,
            essence_success_rate,
            firecrawl_success_rate,
            quality_win_rate,
            speed_win_rate,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Batch-insert URL results for a run.
/// `comparisons` should be serializable UrlComparison-like structs.
/// We accept raw serde_json::Value to avoid coupling to the benchmark's types.
pub fn insert_url_results(
    conn: &Connection,
    run_id: i64,
    comparisons: &[serde_json::Value],
) -> SqliteResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO url_results
                (run_id, url, category, description,
                 e_success, e_word_count, e_markdown_length, e_heading_count,
                 e_link_count, e_image_count, e_code_block_count, e_table_count,
                 e_html_artifact_count, e_empty_link_count, e_base64_count,
                 e_content_density, e_has_title, e_has_description,
                 e_response_time_ms, e_content_hash,
                 f_success, f_word_count, f_markdown_length, f_heading_count,
                 f_link_count, f_image_count, f_code_block_count, f_table_count,
                 f_html_artifact_count, f_empty_link_count, f_base64_count,
                 f_content_density, f_has_title, f_has_description,
                 f_response_time_ms, f_content_hash,
                 overall_winner, essence_advantage,
                 quality_winner, speed_winner,
                 w_word_count, w_heading_preservation, w_link_preservation,
                 w_image_preservation, w_code_block_preservation,
                 w_markdown_cleanliness, w_content_density,
                 w_metadata_extraction, w_speed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
                     ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36,
                     ?37,?38,?39,?40,?41,?42,?43,?44,?45,?46,?47,?48,?49)",
        )?;

        for c in comparisons {
            let e = &c["essence"];
            let f = &c["firecrawl"];
            let w = &c["winners"];

            stmt.execute(params![
                run_id,
                c["url"].as_str().unwrap_or(""),
                c["category"].as_str().unwrap_or(""),
                c["description"].as_str().unwrap_or(""),
                e["success"].as_bool().unwrap_or(false) as i64,
                e["word_count"].as_i64().unwrap_or(0),
                e["markdown_length"].as_i64().unwrap_or(0),
                e["heading_count"].as_i64().unwrap_or(0),
                e["link_count"].as_i64().unwrap_or(0),
                e["image_count"].as_i64().unwrap_or(0),
                e["code_block_count"].as_i64().unwrap_or(0),
                e["table_count"].as_i64().unwrap_or(0),
                e["html_artifact_count"].as_i64().unwrap_or(0),
                e["empty_link_count"].as_i64().unwrap_or(0),
                e["base64_count"].as_i64().unwrap_or(0),
                e["content_density"].as_f64().unwrap_or(0.0),
                e["has_title"].as_bool().unwrap_or(false) as i64,
                e["has_description"].as_bool().unwrap_or(false) as i64,
                e["response_time_ms"].as_i64().unwrap_or(0),
                e["content_hash"].as_str().unwrap_or(""),
                f["success"].as_bool().unwrap_or(false) as i64,
                f["word_count"].as_i64().unwrap_or(0),
                f["markdown_length"].as_i64().unwrap_or(0),
                f["heading_count"].as_i64().unwrap_or(0),
                f["link_count"].as_i64().unwrap_or(0),
                f["image_count"].as_i64().unwrap_or(0),
                f["code_block_count"].as_i64().unwrap_or(0),
                f["table_count"].as_i64().unwrap_or(0),
                f["html_artifact_count"].as_i64().unwrap_or(0),
                f["empty_link_count"].as_i64().unwrap_or(0),
                f["base64_count"].as_i64().unwrap_or(0),
                f["content_density"].as_f64().unwrap_or(0.0),
                f["has_title"].as_bool().unwrap_or(false) as i64,
                f["has_description"].as_bool().unwrap_or(false) as i64,
                f["response_time_ms"].as_i64().unwrap_or(0),
                f["content_hash"].as_str().unwrap_or(""),
                c["overall_winner"].as_str().unwrap_or("tie"),
                c["essence_advantage"].as_f64().unwrap_or(0.0),
                c["quality_winner"].as_str().unwrap_or("pending"),
                c["speed_winner"].as_str().unwrap_or("tie"),
                w["word_count"].as_str().unwrap_or("tie"),
                w["heading_preservation"].as_str().unwrap_or("tie"),
                w["link_preservation"].as_str().unwrap_or("tie"),
                w["image_preservation"].as_str().unwrap_or("tie"),
                w["code_block_preservation"].as_str().unwrap_or("tie"),
                w["markdown_cleanliness"].as_str().unwrap_or("tie"),
                w["content_density"].as_str().unwrap_or("tie"),
                w["metadata_extraction"].as_str().unwrap_or("tie"),
                w["speed"].as_str().unwrap_or("tie"),
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Insert an LLM verdict for a single URL in a run
pub fn insert_llm_verdict(
    conn: &Connection,
    run_id: i64,
    url: &str,
    verdict: &serde_json::Value,
    model: &str,
    eval_time_ms: u128,
) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO llm_verdicts
            (run_id, url,
             content_relevance_winner, content_relevance_reasoning,
             noise_removal_winner, noise_removal_reasoning,
             readability_winner, readability_reasoning,
             structural_coherence_winner, structural_coherence_reasoning,
             information_completeness_winner, information_completeness_reasoning,
             token_efficiency_winner, token_efficiency_reasoning,
             overall_llm_winner, overall_llm_reasoning,
             model_used, evaluation_time_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            run_id,
            url,
            verdict["content_relevance"]["winner"]
                .as_str()
                .unwrap_or("tie"),
            verdict["content_relevance"]["reasoning"]
                .as_str()
                .unwrap_or(""),
            verdict["noise_removal"]["winner"].as_str().unwrap_or("tie"),
            verdict["noise_removal"]["reasoning"].as_str().unwrap_or(""),
            verdict["readability"]["winner"].as_str().unwrap_or("tie"),
            verdict["readability"]["reasoning"].as_str().unwrap_or(""),
            verdict["structural_coherence"]["winner"]
                .as_str()
                .unwrap_or("tie"),
            verdict["structural_coherence"]["reasoning"]
                .as_str()
                .unwrap_or(""),
            verdict["information_completeness"]["winner"]
                .as_str()
                .unwrap_or("tie"),
            verdict["information_completeness"]["reasoning"]
                .as_str()
                .unwrap_or(""),
            verdict["token_efficiency"]["winner"]
                .as_str()
                .unwrap_or("tie"),
            verdict["token_efficiency"]["reasoning"]
                .as_str()
                .unwrap_or(""),
            verdict["overall_winner"].as_str().unwrap_or("tie"),
            verdict["overall_reasoning"].as_str().unwrap_or(""),
            model,
            eval_time_ms as i64,
        ],
    )?;
    Ok(())
}

// MARK: - Query Functions (for dashboard)

/// Load all run summaries ordered by timestamp
pub fn load_all_runs(conn: &Connection) -> SqliteResult<Vec<RunSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, git_commit, total_urls,
                essence_wins, firecrawl_wins, ties,
                essence_win_rate, essence_success_rate, firecrawl_success_rate, notes,
                quality_win_rate, speed_win_rate
         FROM benchmark_runs ORDER BY id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RunSummary {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            git_commit: row.get(2)?,
            total_urls: row.get(3)?,
            essence_wins: row.get(4)?,
            firecrawl_wins: row.get(5)?,
            ties: row.get(6)?,
            essence_win_rate: row.get(7)?,
            essence_success_rate: row.get(8)?,
            firecrawl_success_rate: row.get(9)?,
            notes: row.get(10)?,
            quality_win_rate: row.get(11).unwrap_or(0.0),
            speed_win_rate: row.get(12).unwrap_or(0.0),
        })
    })?;

    rows.collect()
}

/// Load URL results for the most recent run
pub fn load_latest_url_results(conn: &Connection) -> SqliteResult<Vec<UrlResultRow>> {
    let mut stmt = conn.prepare(
        "SELECT url, category, description,
                e_success, e_word_count, e_markdown_length, e_heading_count,
                e_link_count, e_image_count, e_code_block_count,
                e_html_artifact_count, e_content_density, e_response_time_ms,
                e_has_title, e_has_description,
                f_success, f_word_count, f_markdown_length, f_heading_count,
                f_link_count, f_image_count, f_code_block_count,
                f_html_artifact_count, f_content_density, f_response_time_ms,
                f_has_title, f_has_description,
                overall_winner, essence_advantage,
                quality_winner, speed_winner,
                w_word_count, w_heading_preservation, w_link_preservation,
                w_image_preservation, w_code_block_preservation,
                w_markdown_cleanliness, w_content_density,
                w_metadata_extraction, w_speed
         FROM url_results
         WHERE run_id = (SELECT MAX(id) FROM benchmark_runs)
         ORDER BY category, url",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(UrlResultRow {
            url: row.get(0)?,
            category: row.get(1)?,
            description: row.get(2)?,
            e_success: row.get::<_, i64>(3)? != 0,
            e_word_count: row.get(4)?,
            e_markdown_length: row.get(5)?,
            e_heading_count: row.get(6)?,
            e_link_count: row.get(7)?,
            e_image_count: row.get(8)?,
            e_code_block_count: row.get(9)?,
            e_html_artifact_count: row.get(10)?,
            e_content_density: row.get(11)?,
            e_response_time_ms: row.get(12)?,
            e_has_title: row.get::<_, i64>(13)? != 0,
            e_has_description: row.get::<_, i64>(14)? != 0,
            f_success: row.get::<_, i64>(15)? != 0,
            f_word_count: row.get(16)?,
            f_markdown_length: row.get(17)?,
            f_heading_count: row.get(18)?,
            f_link_count: row.get(19)?,
            f_image_count: row.get(20)?,
            f_code_block_count: row.get(21)?,
            f_html_artifact_count: row.get(22)?,
            f_content_density: row.get(23)?,
            f_response_time_ms: row.get(24)?,
            f_has_title: row.get::<_, i64>(25)? != 0,
            f_has_description: row.get::<_, i64>(26)? != 0,
            overall_winner: row.get(27)?,
            essence_advantage: row.get(28)?,
            quality_winner: row
                .get::<_, String>(29)
                .unwrap_or_else(|_| "pending".to_string()),
            speed_winner: row
                .get::<_, String>(30)
                .unwrap_or_else(|_| "tie".to_string()),
            w_word_count: row.get(31)?,
            w_heading_preservation: row.get(32)?,
            w_link_preservation: row.get(33)?,
            w_image_preservation: row.get(34)?,
            w_code_block_preservation: row.get(35)?,
            w_markdown_cleanliness: row.get(36)?,
            w_content_density: row.get(37)?,
            w_metadata_extraction: row.get(38)?,
            w_speed: row.get(39)?,
        })
    })?;

    rows.collect()
}

/// Load LLM verdicts for the most recent run
pub fn load_latest_llm_verdicts(conn: &Connection) -> SqliteResult<Vec<LlmVerdictRow>> {
    let mut stmt = conn.prepare(
        "SELECT url,
                content_relevance_winner, content_relevance_reasoning,
                noise_removal_winner, noise_removal_reasoning,
                readability_winner, readability_reasoning,
                structural_coherence_winner, structural_coherence_reasoning,
                information_completeness_winner, information_completeness_reasoning,
                token_efficiency_winner, token_efficiency_reasoning,
                overall_llm_winner, overall_llm_reasoning, model_used
         FROM llm_verdicts
         WHERE run_id = (SELECT MAX(id) FROM benchmark_runs)
         ORDER BY url",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(LlmVerdictRow {
            url: row.get(0)?,
            content_relevance_winner: row.get(1)?,
            content_relevance_reasoning: row.get(2)?,
            noise_removal_winner: row.get(3)?,
            noise_removal_reasoning: row.get(4)?,
            readability_winner: row.get(5)?,
            readability_reasoning: row.get(6)?,
            structural_coherence_winner: row.get(7)?,
            structural_coherence_reasoning: row.get(8)?,
            information_completeness_winner: row.get(9)?,
            information_completeness_reasoning: row.get(10)?,
            token_efficiency_winner: row.get(11).unwrap_or_else(|_| "tie".to_string()),
            token_efficiency_reasoning: row.get(12).unwrap_or_else(|_| String::new()),
            overall_llm_winner: row.get(13)?,
            overall_llm_reasoning: row.get(14)?,
            model_used: row.get(15)?,
        })
    })?;

    rows.collect()
}

/// Compute per-dimension win rates across all runs (for trend charts)
pub fn load_dimension_trends(
    conn: &Connection,
) -> SqliteResult<std::collections::HashMap<String, Vec<(i64, f64)>>> {
    use std::collections::HashMap;

    let dimensions = [
        "w_word_count",
        "w_heading_preservation",
        "w_link_preservation",
        "w_image_preservation",
        "w_code_block_preservation",
        "w_markdown_cleanliness",
        "w_content_density",
        "w_metadata_extraction",
        "w_speed",
    ];

    let mut result: HashMap<String, Vec<(i64, f64)>> = HashMap::new();

    for dim in &dimensions {
        let query = format!(
            "SELECT r.id,
                    CAST(SUM(CASE WHEN u.{dim} = 'essence' THEN 1 ELSE 0 END) AS REAL)
                    / CAST(COUNT(*) AS REAL) AS win_rate
             FROM url_results u
             JOIN benchmark_runs r ON u.run_id = r.id
             GROUP BY r.id
             ORDER BY r.id ASC"
        );

        let mut stmt = conn.prepare(&query)?;
        let rows: Vec<(i64, f64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let clean_name = dim.strip_prefix("w_").unwrap_or(dim).to_string();
        result.insert(clean_name, rows);
    }

    Ok(result)
}

/// Compute per-category win rates for the latest run
pub fn load_category_win_rates(
    conn: &Connection,
) -> SqliteResult<std::collections::HashMap<String, f64>> {
    let mut stmt = conn.prepare(
        "SELECT category,
                CAST(SUM(CASE WHEN overall_winner = 'essence' THEN 1 ELSE 0 END) AS REAL)
                / CAST(COUNT(*) AS REAL) AS win_rate
         FROM url_results
         WHERE run_id = (SELECT MAX(id) FROM benchmark_runs)
         GROUP BY category
         ORDER BY category",
    )?;

    let rows: std::collections::HashMap<String, f64> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Load average response time per engine per run (for speed trend chart)
pub fn load_speed_trends(conn: &Connection) -> SqliteResult<Vec<RunSpeedTrend>> {
    let mut stmt = conn.prepare(
        "SELECT r.id,
                AVG(u.e_response_time_ms) AS avg_e,
                AVG(u.f_response_time_ms) AS avg_f
         FROM url_results u
         JOIN benchmark_runs r ON u.run_id = r.id
         GROUP BY r.id
         ORDER BY r.id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RunSpeedTrend {
            run_id: row.get(0)?,
            avg_essence_ms: row.get(1)?,
            avg_firecrawl_ms: row.get(2)?,
        })
    })?;

    rows.collect()
}

/// Load average content metrics per engine per run (for metric trend chart)
pub fn load_metric_trends(conn: &Connection) -> SqliteResult<Vec<RunMetricTrend>> {
    let mut stmt = conn.prepare(
        "SELECT r.id,
                AVG(u.e_word_count),
                AVG(u.f_word_count),
                AVG(u.e_heading_count),
                AVG(u.f_heading_count),
                AVG(u.e_html_artifact_count),
                AVG(u.f_html_artifact_count),
                CAST(SUM(u.e_success) AS REAL) / CAST(COUNT(*) AS REAL),
                CAST(SUM(u.f_success) AS REAL) / CAST(COUNT(*) AS REAL)
         FROM url_results u
         JOIN benchmark_runs r ON u.run_id = r.id
         GROUP BY r.id
         ORDER BY r.id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RunMetricTrend {
            run_id: row.get(0)?,
            avg_e_word_count: row.get(1)?,
            avg_f_word_count: row.get(2)?,
            avg_e_headings: row.get(3)?,
            avg_f_headings: row.get(4)?,
            avg_e_artifacts: row.get(5)?,
            avg_f_artifacts: row.get(6)?,
            essence_success_rate: row.get(7)?,
            firecrawl_success_rate: row.get(8)?,
        })
    })?;

    rows.collect()
}

/// Load per-category win rates across all runs (for category trend chart)
pub fn load_category_trends(
    conn: &Connection,
) -> SqliteResult<std::collections::HashMap<String, Vec<(i64, f64)>>> {
    use std::collections::HashMap;

    let mut stmt = conn.prepare(
        "SELECT r.id, u.category,
                CAST(SUM(CASE WHEN u.overall_winner = 'essence' THEN 1 ELSE 0 END) AS REAL)
                / CAST(COUNT(*) AS REAL) AS win_rate
         FROM url_results u
         JOIN benchmark_runs r ON u.run_id = r.id
         GROUP BY r.id, u.category
         ORDER BY r.id ASC, u.category",
    )?;

    let mut result: HashMap<String, Vec<(i64, f64)>> = HashMap::new();

    stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?
    .filter_map(|r| r.ok())
    .for_each(|(run_id, category, rate)| {
        result.entry(category).or_default().push((run_id, rate));
    });

    Ok(result)
}

/// Load quality (LLM) win rate trend over time
pub fn load_quality_trend(conn: &Connection) -> SqliteResult<Vec<(i64, f64)>> {
    let mut stmt =
        conn.prepare("SELECT id, quality_win_rate FROM benchmark_runs ORDER BY id ASC")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get::<_, f64>(1).unwrap_or(0.0)))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Load all data needed by the dashboard generator
pub fn load_dashboard_data(conn: &Connection) -> SqliteResult<DashboardData> {
    Ok(DashboardData {
        runs: load_all_runs(conn)?,
        latest_url_results: load_latest_url_results(conn)?,
        latest_llm_verdicts: load_latest_llm_verdicts(conn)?,
        dimension_trends: load_dimension_trends(conn)?,
        category_win_rates: load_category_win_rates(conn)?,
        speed_trends: load_speed_trends(conn)?,
        metric_trends: load_metric_trends(conn)?,
        category_trends: load_category_trends(conn)?,
        quality_trend: load_quality_trend(conn)?,
    })
}
