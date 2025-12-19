//! WebP Converter Module
//! Downloads images and converts them to WebP format

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::{DynamicImage, ImageFormat, ImageError};
use std::io::Cursor;

/// Result of WebP conversion
#[derive(Debug, Clone)]
pub struct ConvertedImage {
    /// Original URL of the image
    pub original_url: String,
    /// Base64-encoded image data (WebP or original)
    pub webp_base64: String,
    /// Suggested filename (hash-based)
    pub filename: String,
    /// Original size in bytes
    pub original_size: usize,
    /// WebP size in bytes
    pub webp_size: usize,
    /// Reduction percentage
    pub reduction_percent: f32,
}

/// WebP conversion result for API response
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebpConversionResult {
    pub images: Vec<ConvertedImageResponse>,
    pub total_original_kb: f32,
    pub total_webp_kb: f32,
    pub total_savings_kb: f32,
    pub average_reduction_percent: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConvertedImageResponse {
    pub original_url: String,
    pub webp_filename: String,
    pub webp_base64: String,
    pub original_size: usize,
    pub webp_size: usize,
    pub reduction_percent: f32,
}

/// Quality setting for WebP conversion (1-100)
const WEBP_QUALITY: u8 = 80;

/// Maximum image dimension (resize if larger)
const MAX_DIMENSION: u32 = 2048;

/// Download an image from a URL
pub async fn download_image(url: &str) -> Result<Vec<u8>, String> {
    tracing::debug!("WebP converter: Downloading image from {}", url);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .header("User-Agent", "HTMLWordPress/1.0")
        .send()
        .await
        .map_err(|e| format!("Failed to download image: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}: {}", response.status(), url));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    tracing::debug!("WebP converter: Downloaded {} bytes from {}", bytes.len(), url);
    Ok(bytes.to_vec())
}

/// Convert image bytes to WebP format
pub fn convert_to_webp(image_data: &[u8], quality: u8, resize: bool) -> Result<Vec<u8>, String> {
    tracing::debug!("WebP converter: Converting {} bytes to WebP (quality={})", image_data.len(), quality);

    // Load the image
    let img = image::load_from_memory(image_data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    // Resize if too large AND enabled
    let img = if resize {
        resize_if_needed(img, MAX_DIMENSION)
    } else {
        img
    };

    // Convert to WebP
    let mut webp_data = Vec::new();
    let mut cursor = Cursor::new(&mut webp_data);
    
    img.write_to(&mut cursor, ImageFormat::WebP)
        .map_err(|e| format!("Failed to encode WebP: {}", e))?;

    tracing::debug!("WebP converter: Converted to {} bytes", webp_data.len());
    Ok(webp_data)
}

/// Resize image if it exceeds max dimension
fn resize_if_needed(img: DynamicImage, max_dim: u32) -> DynamicImage {
    let (width, height) = (img.width(), img.height());
    
    if width > max_dim || height > max_dim {
        tracing::debug!("WebP converter: Resizing {}x{} to max {}", width, height, max_dim);
        img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
    } else {
        img
    }
}

/// Generate a hash-based filename
fn generate_filename(url: &str, extension: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{:x}.{}", hash, extension)
}

/// Convert a single image from URL to WebP
pub async fn convert_image_url(url: &str, base_url: &str, resize: bool) -> Result<ConvertedImage, String> {
    // Make URL absolute if relative
    let full_url = if url.starts_with("/") {
        format!("{}{}", base_url.trim_end_matches('/'), url)
    } else if url.starts_with("http") {
        url.to_string()
    } else {
        format!("{}/{}", base_url.trim_end_matches('/'), url)
    };

    // Download the image
    let original_data = download_image(&full_url).await?;
    let original_size = original_data.len();

    // Convert to WebP
    let webp_data = convert_to_webp(&original_data, WEBP_QUALITY, resize)?;
    let webp_size = webp_data.len();

    // If WebP is larger (or equal), use ORIGINAL
    if webp_size >= original_size {
        tracing::info!(
            "WebP converter: Skipping conversion for {} - WebP larger ({} -> {}). Using original.",
            url, original_size, webp_size
        );
        
        let extension = if url.to_lowercase().ends_with(".png") { "png" } else { "jpg" };
        let filename = generate_filename(url, extension);
        let base64_data = BASE64.encode(&original_data);

        return Ok(ConvertedImage {
            original_url: url.to_string(),
            webp_base64: base64_data,
            filename,
            original_size,
            webp_size: original_size, // Effectively the same
            reduction_percent: 0.0,
        });
    }

    // Calculate reduction
    let reduction = ((original_size - webp_size) as f32 / original_size as f32) * 100.0;

    // Base64 encode
    let webp_base64 = BASE64.encode(&webp_data);

    tracing::info!(
        "WebP converter: {} -> {} bytes ({:.1}% reduction)",
        original_size, webp_size, reduction
    );

    Ok(ConvertedImage {
        original_url: url.to_string(),
        webp_base64,
        filename: generate_filename(url, "webp"),
        original_size,
        webp_size,
        reduction_percent: reduction,
    })
}

/// Extract image URLs from HTML and convert them to WebP
pub async fn convert_images_in_html(html: &str, base_url: &str, resize: bool) -> WebpConversionResult {
    tracing::info!("WebP converter: Starting image extraction from HTML");
    
    let mut images = Vec::new();
    let mut total_original: usize = 0;
    let mut total_webp: usize = 0;

    // Extract image URLs using regex-like approach
    let image_urls = extract_image_urls(html);
    
    tracing::debug!("WebP converter: Found {} image URLs", image_urls.len());

    for url in image_urls {
        // Skip small icons, SVGs, data URLs
        if should_skip_image(&url) {
            tracing::debug!("WebP converter: Skipping {}", url);
            continue;
        }

        match convert_image_url(&url, base_url, resize).await {
            Ok(converted) => {
                total_original += converted.original_size;
                total_webp += converted.webp_size;
                
                images.push(ConvertedImageResponse {
                    original_url: converted.original_url,
                    webp_filename: converted.filename,
                    webp_base64: converted.webp_base64,
                    original_size: converted.original_size,
                    webp_size: converted.webp_size,
                    reduction_percent: converted.reduction_percent,
                });
            }
            Err(e) => {
                tracing::warn!("WebP converter: Failed to convert {}: {}", url, e);
            }
        }
    }

    let total_savings = total_original.saturating_sub(total_webp);
    let avg_reduction = if !images.is_empty() {
        images.iter().map(|i| i.reduction_percent).sum::<f32>() / images.len() as f32
    } else {
        0.0
    };

    tracing::info!(
        "WebP converter: Converted {} images, saved {:.1} KB ({:.1}% avg reduction)",
        images.len(),
        total_savings as f32 / 1024.0,
        avg_reduction
    );

    WebpConversionResult {
        images,
        total_original_kb: total_original as f32 / 1024.0,
        total_webp_kb: total_webp as f32 / 1024.0,
        total_savings_kb: total_savings as f32 / 1024.0,
        average_reduction_percent: avg_reduction,
    }
}

/// Extract image URLs from HTML (src and srcset)
fn extract_image_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for src="
        if i + 5 <= len {
            let tag: String = chars[i..i+5].iter().collect();
            if tag.to_lowercase() == "src=\"" || tag.to_lowercase() == "src='" {
                let quote_char = chars[i+4];
                i += 5;
                let url_start = i;
                while i < len && chars[i] != quote_char {
                    i += 1;
                }
                let url: String = chars[url_start..i].iter().collect();
                if is_image_url(&url) {
                    urls.push(url);
                }
                continue;
            }
        }

        // Look for srcset="
        if i + 8 <= len {
            let tag: String = chars[i..i+8].iter().collect();
            if tag.to_lowercase() == "srcset=\"" || tag.to_lowercase() == "srcset='" {
                let quote_char = chars[i+7];
                i += 8;
                let val_start = i;
                while i < len && chars[i] != quote_char {
                    i += 1;
                }
                let srcset_val: String = chars[val_start..i].iter().collect();
                // tracing::debug!("WebP converter: Parse srcset: '{}'", srcset_val);
                
                // Parse srcset: "url1 1x, url2 2x"
                for part in srcset_val.split(',') {
                    let part = part.trim();
                    if let Some(url_end) = part.find(' ') {
                        let url = &part[..url_end];
                        if is_image_url(url) {
                            // tracing::debug!("WebP converter: Found srcset URL: {}", url);
                            urls.push(url.to_string());
                        }
                    } else if !part.is_empty() {
                         // Fallback for no descriptor
                         if is_image_url(part) {
                            // tracing::debug!("WebP converter: Found srcset URL (no descriptor): {}", part);
                            urls.push(part.to_string());
                         }
                    }
                }
                continue;
            }
        }
        
        i += 1;
    }

    // Dedup
    urls.sort();
    urls.dedup();
    urls
}

/// Check if URL is an image
fn is_image_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.ends_with(".jpg") || 
    lower.ends_with(".jpeg") || 
    lower.ends_with(".png") || 
    lower.ends_with(".gif") ||
    lower.ends_with(".webp")
}

/// Check if image should be skipped (already WebP, SVG, data URL, etc.)
fn should_skip_image(url: &str) -> bool {
    let lower = url.to_lowercase();
    
    // Skip data URLs
    if url.starts_with("data:") {
        return true;
    }
    
    // Skip already WebP
    if lower.ends_with(".webp") {
        return true;
    }
    
    // Skip SVGs
    if lower.ends_with(".svg") {
        return true;
    }
    
    // Skip very small images (icons)
    if lower.contains("favicon") || lower.contains("icon") {
        return true;
    }
    
    false
}

/// Rewrite HTML to use local WebP paths
pub fn rewrite_html_with_webp(html: &mut String, images: &[ConvertedImageResponse], upload_base_url: &str) {
    for image in images {
        let webp_url = format!("{}/images/{}", upload_base_url.trim_end_matches('/'), image.webp_filename);
        
        // Replace old URL with new WebP URL
        *html = html.replace(&image.original_url, &webp_url);
        
        tracing::debug!("WebP rewrite: {} -> {}", image.original_url, webp_url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_image_urls() {
        let html = r#"<img src="/uploads/test.jpg"><img src="/images/photo.png" srcset="/images/photo-2x.png 2x, /images/photo-sm.png 500w">"#;
        let urls = extract_image_urls(html);
        assert_eq!(urls.len(), 4);
        assert!(urls.contains(&"/uploads/test.jpg".to_string()));
        assert!(urls.contains(&"/images/photo-2x.png".to_string()));
    }

    #[test]
    fn test_should_skip_image() {
        assert!(should_skip_image("data:image/png;base64,..."));
        assert!(should_skip_image("/images/favicon.ico"));
        assert!(should_skip_image("/images/logo.webp"));
        assert!(!should_skip_image("/uploads/photo.jpg"));
    }

    #[test]
    fn test_generate_filename() {
        let filename = generate_filename("/uploads/test.jpg", "webp");
        assert!(filename.ends_with(".webp"));
        assert!(filename.len() > 10);
    }
}
