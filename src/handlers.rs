//! API Handlers

use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::config::AppState;
use crate::optimizer;

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
    auth_enabled: bool,
}

/// Health check endpoint
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        auth_enabled: state.api_key.is_some(),
    })
}

/// Optimization request
#[derive(Deserialize)]
pub struct OptimizeRequest {
    pub html: String,
    pub url: String,
    #[serde(default)]
    pub options: OptimizeOptions,
}

#[derive(Deserialize)]
pub struct OptimizeOptions {
    #[serde(default = "default_true")]
    pub minify_html: bool,
    #[serde(default = "default_true")]
    pub minify_css: bool,
    #[serde(default = "default_true")]
    pub minify_js: bool,
    #[serde(default = "default_true")]
    pub remove_unused_css: bool,
    #[serde(default = "default_true")]
    pub convert_webp: bool,
    #[serde(default = "default_true")]
    pub resize_images: bool,
    #[serde(default = "default_true")]
    pub defer_js: bool,
    #[serde(default = "default_true")]
    pub lazy_images: bool,
    #[serde(default = "default_true")]
    pub optimize_resources: bool,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        Self {
            minify_html: true,
            minify_css: true,
            minify_js: true,
            remove_unused_css: true,
            convert_webp: true,
            resize_images: true,
            defer_js: true,
            lazy_images: true,
            optimize_resources: true,
        }
    }
}

fn default_level() -> String {
    "balanced".to_string()
}

fn default_true() -> bool {
    true
}

/// Optimization response
#[derive(Serialize)]
pub struct OptimizeResponse {
    pub success: bool,
    pub optimized_html: String,
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f64,
    pub optimizations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<WebpImagesResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesResponse>,
}

/// WebP images response
#[derive(Serialize)]
pub struct WebpImagesResponse {
    pub images: Vec<WebpImageData>,
    pub total_original_kb: f32,
    pub total_webp_kb: f32,
    pub total_savings_kb: f32,
}

#[derive(Serialize)]
pub struct WebpImageData {
    pub original_url: String,
    pub webp_filename: String,
    pub webp_base64: String,
    pub original_size: usize,
    pub webp_size: usize,
    pub reduction_percent: f32,
}

/// Optimized CSS/JS resources response
#[derive(Serialize)]
pub struct ResourcesResponse {
    pub css_files: Vec<CssFileData>,
    pub js_files: Vec<JsFileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical_css: Option<String>,
    /// Combined CSS - all CSS merged into one file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combined_css: Option<String>,
    /// Combined JS - all JS merged into one file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combined_js: Option<String>,
    pub combined_css_filename: String,
    pub combined_js_filename: String,
    pub total_css_savings_kb: f32,
    pub total_js_savings_kb: f32,
}

#[derive(Serialize)]
pub struct CssFileData {
    pub original_url: String,
    pub filename: String,
    pub content: String,
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f32,
}

#[derive(Serialize)]
pub struct JsFileData {
    pub original_url: String,
    pub filename: String,
    pub content: String,
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f32,
}

/// Single page optimization
pub async fn optimize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OptimizeRequest>,
) -> Result<Json<OptimizeResponse>, AppError> {
    // Check API Key
    if let Some(ref key) = state.api_key {
        let auth_header = headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        
        if auth_header != format!("Bearer {}", key) {
            return Err(AppError::Unauthorized);
        }
    } else {
        tracing::error!("Security Error: No API Key configured on server");
        return Err(AppError::Internal("Server misconfiguration: API_KEY must be set".to_string()));
    }

    if req.html.is_empty() {
        return Err(AppError::BadRequest("HTML is required".to_string()));
    }

    tracing::info!("Optimizing: {} ({} bytes)", req.url, req.html.len());

    let mut result = optimizer::optimize_html(&req.html, &req.url, &req.options)?;

    // WebP conversion if enabled
    let images = if req.options.convert_webp {
        tracing::info!("WebP conversion: Starting for {}", req.url);
        let webp_result = crate::webp_converter::convert_images_in_html(&result.html, &req.url, req.options.resize_images).await;
        
        if !webp_result.images.is_empty() {
            // Rewrite HTML with placeholder paths (WordPress will replace with actual paths)
            let upload_base = format!("{}/wp-content/uploads", req.url.trim_end_matches('/'));
            crate::webp_converter::rewrite_html_with_webp(&mut result.html, &webp_result.images, &upload_base);
            
            result.optimizations.push(format!(
                "{} images converted to WebP (saved {:.1} KB)",
                webp_result.images.len(),
                webp_result.total_savings_kb
            ));

            Some(WebpImagesResponse {
                images: webp_result.images.into_iter().map(|img| WebpImageData {
                    original_url: img.original_url,
                    webp_filename: img.webp_filename,
                    webp_base64: img.webp_base64,
                    original_size: img.original_size,
                    webp_size: img.webp_size,
                    reduction_percent: img.reduction_percent,
                }).collect(),
                total_original_kb: webp_result.total_original_kb,
                total_webp_kb: webp_result.total_webp_kb,
                total_savings_kb: webp_result.total_savings_kb,
            })
        } else {
            None
        }
    } else {
        None
    };

    // External resource optimization if enabled
    let resources = if req.options.optimize_resources {
        tracing::info!("Resource optimization: Starting for {}", req.url);
        
        // Get used selectors from CSS optimizer for tree-shaking
        let used_selectors = crate::css_optimizer::CssOptimizer::extract_used_selectors_static(&result.html);
        let res_result = crate::resource_optimizer::optimize_external_resources(&result.html, &req.url, &used_selectors, &req.options).await;
        
        if !res_result.css_files.is_empty() || !res_result.js_files.is_empty() {
            // Rewrite HTML with local paths
            let upload_base = format!("{}/wp-content/uploads", req.url.trim_end_matches('/'));
            crate::resource_optimizer::rewrite_html_with_optimized_resources(&mut result.html, &res_result, &upload_base);
            
            result.optimizations.push(format!(
                "{} CSS files optimized (saved {:.1} KB), {} JS files optimized (saved {:.1} KB)",
                res_result.css_files.len(), res_result.total_css_savings_kb,
                res_result.js_files.len(), res_result.total_js_savings_kb
            ));
            
            if res_result.critical_css.is_some() {
                result.optimizations.push("Critical CSS extracted and inlined".to_string());
            }

            Some(ResourcesResponse {
                css_files: res_result.css_files.into_iter().map(|f| CssFileData {
                    original_url: f.original_url,
                    filename: f.filename,
                    content: f.content,
                    original_size: f.original_size,
                    optimized_size: f.optimized_size,
                    reduction_percent: f.reduction_percent,
                }).collect(),
                js_files: res_result.js_files.into_iter().map(|f| JsFileData {
                    original_url: f.original_url,
                    filename: f.filename,
                    content: f.content,
                    original_size: f.original_size,
                    optimized_size: f.optimized_size,
                    reduction_percent: f.reduction_percent,
                }).collect(),
                critical_css: res_result.critical_css,
                combined_css: res_result.combined_css,
                combined_js: res_result.combined_js,
                combined_css_filename: res_result.combined_css_filename,
                combined_js_filename: res_result.combined_js_filename,
                total_css_savings_kb: res_result.total_css_savings_kb,
                total_js_savings_kb: res_result.total_js_savings_kb,
            })
        } else {
            None
        }
    } else {
        None
    };

    let response = OptimizeResponse {
        success: true,
        optimized_html: result.html,
        original_size: result.original_size,
        optimized_size: result.optimized_size,
        reduction_percent: result.reduction_percent,
        optimizations: result.optimizations,
        images,
        resources,
    };

    tracing::info!(
        "Optimized: {} -> {} bytes ({:.1}% reduction)",
        response.original_size,
        response.optimized_size,
        response.reduction_percent
    );

    Ok(Json(response))
}

/// Bulk optimization request
#[derive(Deserialize)]
pub struct BulkOptimizeRequest {
    pub pages: Vec<OptimizeRequest>,
}

#[derive(Serialize)]
pub struct BulkOptimizeResponse {
    pub success: bool,
    pub results: Vec<OptimizeResponse>,
    pub total_reduction: f64,
}

/// Bulk optimization endpoint
pub async fn optimize_bulk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BulkOptimizeRequest>,
) -> Result<Json<BulkOptimizeResponse>, AppError> {
    // Check API Key
    if let Some(ref key) = state.api_key {
        let auth_header = headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        
        if auth_header != format!("Bearer {}", key) {
            return Err(AppError::Unauthorized);
        }
    } else {
        tracing::error!("Security Error: No API Key configured on server");
        return Err(AppError::Internal("Server misconfiguration: API_KEY must be set".to_string()));
    }

    let mut results = Vec::new();
    let mut total_original = 0usize;
    let mut total_optimized = 0usize;

    for page in req.pages {
        match optimizer::optimize_html(&page.html, &page.url, &page.options) {
            Ok(result) => {
                total_original += result.original_size;
                total_optimized += result.optimized_size;

                results.push(OptimizeResponse {
                    success: true,
                    optimized_html: result.html,
                    original_size: result.original_size,
                    optimized_size: result.optimized_size,
                    reduction_percent: result.reduction_percent,
                    optimizations: result.optimizations,
                    images: None,
                    resources: None,
                });
            }
            Err(e) => {
                tracing::warn!("Failed to optimize {}: {}", page.url, e);
                results.push(OptimizeResponse {
                    success: false,
                    optimized_html: page.html,
                    original_size: 0,
                    optimized_size: 0,
                    reduction_percent: 0.0,
                    optimizations: vec![],
                    images: None,
                    resources: None,
                });
            }
        }
    }

    let total_reduction = if total_original > 0 {
        (1.0 - (total_optimized as f64 / total_original as f64)) * 100.0
    } else {
        0.0
    };

    Ok(Json(BulkOptimizeResponse {
        success: true,
        results,
        total_reduction,
    }))
}
