use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScrapeError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Timeout occurred")]
    Timeout,

    #[error("Failed to parse HTML: {0}")]
    ParseError(String),

    #[error("Robots.txt disallows scraping")]
    RobotsDisallowed,

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Browser error: {0}")]
    BrowserError(String),

    #[error("Browser launch failed: {0}")]
    BrowserLaunchFailed(String),

    #[error("Navigation failed: {0}")]
    NavigationFailed(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("Validation failed")]
    ValidationFailed(Vec<String>),

    #[error("Browser not found: {0}")]
    BrowserNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("SSRF attempt detected: {0}")]
    SsrfAttempt(String),

    #[error("Empty content: {0}")]
    EmptyContent(String),

    #[error("Low quality content: {0}")]
    LowQuality(String),

    #[error("Error page detected: {0}")]
    ErrorPage(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

impl IntoResponse for ScrapeError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ScrapeError::RequestFailed(ref e) => {
                if e.is_timeout() {
                    (StatusCode::REQUEST_TIMEOUT, "Request timeout".to_string())
                } else if e.is_connect() {
                    (
                        StatusCode::BAD_GATEWAY,
                        "Failed to connect to target".to_string(),
                    )
                } else {
                    (StatusCode::BAD_GATEWAY, format!("Request failed: {}", e))
                }
            }
            ScrapeError::InvalidUrl(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ScrapeError::Timeout => (StatusCode::REQUEST_TIMEOUT, self.to_string()),
            ScrapeError::ParseError(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            ScrapeError::RobotsDisallowed => (StatusCode::FORBIDDEN, self.to_string()),
            ScrapeError::UnsupportedFormat(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ScrapeError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ScrapeError::BrowserError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ScrapeError::BrowserLaunchFailed(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
            }
            ScrapeError::NavigationFailed(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            ScrapeError::ElementNotFound(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            ScrapeError::ValidationFailed(ref errors) => (
                StatusCode::BAD_REQUEST,
                format!("Validation failed: {}", errors.join(", ")),
            ),
            ScrapeError::BrowserNotFound(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
            }
            ScrapeError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ScrapeError::ResourceLimit(_) => (StatusCode::PAYLOAD_TOO_LARGE, self.to_string()),
            ScrapeError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            ScrapeError::SsrfAttempt(_) => (StatusCode::FORBIDDEN, self.to_string()),
            ScrapeError::EmptyContent(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            ScrapeError::LowQuality(_) => (StatusCode::UNPROCESSABLE_ENTITY, self.to_string()),
            ScrapeError::ErrorPage(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            ScrapeError::Configuration(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(json!({
            "success": false,
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

impl ScrapeError {
    /// Returns true if this error is transient and the operation should be retried.
    pub fn is_transient(&self) -> bool {
        matches!(self, ScrapeError::Timeout | ScrapeError::RequestFailed(_))
    }
}

pub type Result<T> = std::result::Result<T, ScrapeError>;
