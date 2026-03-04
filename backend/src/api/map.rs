use crate::{
    crawler::mapper,
    error::ScrapeError,
    types::{MapRequest, MapResponse},
    validation,
};
use axum::Json;
use tracing::{error, info};

/// Handler for POST /api/v1/map
pub async fn map_handler(
    Json(request): Json<MapRequest>,
) -> Result<Json<MapResponse>, ScrapeError> {
    info!("Map request received for URL: {}", request.url);

    // Validate request (includes SSRF protection)
    validation::validate_map_request(&request).await?;

    // Discover URLs
    let links = mapper::discover_urls(&request.url, &request)
        .await
        .map_err(|e| {
            error!("Failed to discover URLs for {}: {}", request.url, e);
            e
        })?;

    info!(
        "Successfully discovered {} URLs for: {}",
        links.len(),
        request.url
    );

    Ok(Json(MapResponse::success(links)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_map_handler_invalid_url() {
        let request = MapRequest {
            url: "".to_string(),
            search: None,
            ignore_sitemap: None,
            include_subdomains: None,
            limit: None,
        };

        let result = map_handler(Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_map_handler_limit_validation() {
        let request = MapRequest {
            url: "https://example.com".to_string(),
            search: None,
            ignore_sitemap: None,
            include_subdomains: None,
            limit: Some(200000), // Exceeds max
        };

        let result = map_handler(Json(request)).await;
        assert!(result.is_err());
    }
}
