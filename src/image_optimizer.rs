//! Image Optimizer Module
//! Handles image optimization hints and WebP detection

use scraper::{Html, Selector};

/// CDN configuration for image optimization
#[derive(Clone)]
pub struct CdnConfig {
    /// CDN provider: "cloudflare", "imgix", "bunny", or "custom"
    pub provider: String,
    /// CDN base URL (e.g., "https://yourdomain.com/cdn-cgi/image" for Cloudflare)
    pub base_url: Option<String>,
    /// Default quality (1-100)
    pub quality: u8,
    /// Default format (webp, avif, auto)
    pub format: String,
}

impl Default for CdnConfig {
    fn default() -> Self {
        Self {
            provider: "cloudflare".to_string(),
            base_url: None,
            quality: 80,
            format: "webp".to_string(),
        }
    }
}

/// Image optimization result
pub struct ImageResult {
    pub optimizations: Vec<String>,
    pub webp_candidates: usize,
    pub missing_dimensions: usize,
    pub missing_lazy: usize,
    pub images_rewritten: usize,
}

/// Rewrite image URLs to use CDN optimization
pub fn rewrite_images_for_cdn(html: &mut String, site_url: &str, cdn_config: &CdnConfig) -> usize {
    tracing::debug!("CDN image rewrite: Starting for site {}", site_url);
    
    let mut count = 0;
    let mut result = String::with_capacity(html.len());
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();

    while i < len {
        // Look for <img or src="
        if i + 3 < len {
            let tag: String = chars[i..i+4].iter().collect();
            
            if tag.to_lowercase() == "src=" {
                // Found src attribute
                let quote_char = if i + 4 < len { chars[i + 4] } else { '"' };
                if quote_char == '"' || quote_char == '\'' {
                    result.push_str("src=");
                    result.push(quote_char);
                    i += 5;
                    
                    // Extract the URL
                    let url_start = i;
                    while i < len && chars[i] != quote_char {
                        i += 1;
                    }
                    
                    let original_url: String = chars[url_start..i].iter().collect();
                    
                    // Check if this is an image URL that should be rewritten
                    if should_rewrite_image(&original_url, site_url) {
                        let new_url = generate_cdn_url(&original_url, site_url, cdn_config);
                        tracing::debug!("CDN rewrite: {} -> {}", original_url, new_url);
                        result.push_str(&new_url);
                        count += 1;
                    } else {
                        result.push_str(&original_url);
                    }
                    
                    if i < len {
                        result.push(chars[i]); // closing quote
                        i += 1;
                    }
                    continue;
                }
            }
        }
        
        result.push(chars[i]);
        i += 1;
    }

    if count > 0 {
        tracing::info!("CDN image rewrite: {} images rewritten to {}", count, cdn_config.provider);
        *html = result;
    } else {
        tracing::debug!("CDN image rewrite: No images to rewrite");
    }

    count
}

/// Check if an image URL should be rewritten for CDN
fn should_rewrite_image(url: &str, site_url: &str) -> bool {
    let url_lower = url.to_lowercase();
    
    // Skip data URLs, SVGs, external images, and already-CDN URLs
    if url.starts_with("data:") || 
       url_lower.ends_with(".svg") ||
       url.contains("cdn-cgi/image") ||
       url.contains("imgix.net") ||
       url.contains("cloudinary.com") {
        return false;
    }
    
    // Only rewrite images with common formats
    let is_image = url_lower.ends_with(".jpg") || 
                   url_lower.ends_with(".jpeg") || 
                   url_lower.ends_with(".png") || 
                   url_lower.ends_with(".gif") ||
                   url_lower.ends_with(".webp");
    
    if !is_image {
        return false;
    }
    
    // For local images, check if they're from the same site
    if url.starts_with("/") || url.starts_with(site_url) || url.contains("wp-content") {
        return true;
    }
    
    false
}

/// Generate a CDN-optimized URL based on provider
fn generate_cdn_url(original_url: &str, site_url: &str, config: &CdnConfig) -> String {
    let full_url = if original_url.starts_with("/") {
        format!("{}{}", site_url.trim_end_matches('/'), original_url)
    } else {
        original_url.to_string()
    };

    match config.provider.as_str() {
        "cloudflare" => {
            // Cloudflare Image Resizing: /cdn-cgi/image/format=webp,quality=80/image.jpg
            let base = site_url.trim_end_matches('/');
            format!(
                "{}/cdn-cgi/image/format={},quality={}/{}",
                base,
                config.format,
                config.quality,
                original_url.trim_start_matches('/')
            )
        }
        "imgix" => {
            // Imgix: https://your-source.imgix.net/image.jpg?auto=format&q=80
            if let Some(base) = &config.base_url {
                format!(
                    "{}/{}?auto=format&q={}&fm={}",
                    base.trim_end_matches('/'),
                    original_url.trim_start_matches('/'),
                    config.quality,
                    config.format
                )
            } else {
                full_url
            }
        }
        "bunny" => {
            // BunnyCDN: Add query params
            format!("{}?width=auto&quality={}", full_url, config.quality)
        }
        _ => full_url,
    }
}

/// Analyze images and add optimization hints
pub fn analyze_images(html: &str) -> ImageResult {
    tracing::debug!("Image analysis: Starting");
    let doc = Html::parse_document(html);
    let mut webp_candidates = 0;
    let mut missing_dimensions = 0;
    let mut missing_lazy = 0;

    if let Ok(selector) = Selector::parse("img") {
        for element in doc.select(&selector) {
            let attrs = element.value();
            
            // Check for WebP conversion candidates
            if let Some(src) = attrs.attr("src") {
                let src_lower = src.to_lowercase();
                if src_lower.ends_with(".jpg") || src_lower.ends_with(".jpeg") || 
                   src_lower.ends_with(".png") || src_lower.ends_with(".gif") {
                    webp_candidates += 1;
                }
            }

            // Check for missing dimensions
            if attrs.attr("width").is_none() && attrs.attr("height").is_none() {
                missing_dimensions += 1;
            }

            // Check for lazy loading
            if attrs.attr("loading").is_none() {
                missing_lazy += 1;
            }
        }
    }

    let mut optimizations = Vec::new();
    
    if webp_candidates > 0 {
        optimizations.push(format!("{} images can be converted to WebP", webp_candidates));
    }
    
    if missing_dimensions > 0 {
        optimizations.push(format!("{} images missing width/height (causes CLS)", missing_dimensions));
    }
    
    tracing::debug!("Image analysis: {} webp candidates, {} missing dimensions, {} missing lazy", 
        webp_candidates, missing_dimensions, missing_lazy);

    ImageResult {
        optimizations,
        webp_candidates,
        missing_dimensions,
        missing_lazy,
        images_rewritten: 0,
    }
}

/// Add WebP <picture> wrapper hints
/// Returns the number of images that could be optimized and suggestions
pub fn suggest_webp_conversion(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut suggestions = Vec::new();

    if let Ok(selector) = Selector::parse("img[src]") {
        for element in doc.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                let src_lower = src.to_lowercase();
                
                // Skip if already WebP or SVG
                if src_lower.ends_with(".webp") || src_lower.ends_with(".svg") {
                    continue;
                }

                // Skip if already in a <picture> element
                if let Some(parent) = element.parent() {
                    if let Some(parent_el) = parent.value().as_element() {
                        if parent_el.name() == "picture" {
                            continue;
                        }
                    }
                }

                // Only suggest for common formats
                if src_lower.ends_with(".jpg") || src_lower.ends_with(".jpeg") || 
                   src_lower.ends_with(".png") {
                    let webp_src = src.rsplit_once('.')
                        .map(|(name, _)| format!("{}.webp", name))
                        .unwrap_or_else(|| format!("{}.webp", src));
                    
                    suggestions.push(format!(
                        "Convert {} to WebP: {}",
                        src, webp_src
                    ));
                }
            }
        }
    }

    suggestions
}

/// Generate responsive image srcset
pub fn suggest_responsive_images(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let mut suggestions = Vec::new();

    if let Ok(selector) = Selector::parse("img[src]:not([srcset])") {
        for element in doc.select(&selector) {
            if let Some(src) = element.value().attr("src") {
                // Skip small images, external images, SVGs
                if src.contains("icon") || src.contains("logo") || 
                   src.ends_with(".svg") || src.starts_with("data:") {
                    continue;
                }

                suggestions.push(format!(
                    "Add srcset to: {} (e.g., srcset=\"{} 1x, {} 2x\")",
                    src, src, src.replace(".", "@2x.")
                ));
            }
        }
    }

    // Limit suggestions
    suggestions.truncate(5);
    suggestions
}

/// Add image dimension hints to HTML (modifies in place)
pub fn add_dimension_hints(html: &mut String) -> usize {
    // For full implementation, we would:
    // 1. Extract all images without dimensions
    // 2. Fetch actual dimensions (requires HTTP client)
    // 3. Add width/height attributes
    
    // For now, we just count and return - actual dimensions would need
    // to be added by the WordPress plugin which has access to attachments
    
    let doc = Html::parse_document(html);
    let mut count = 0;

    if let Ok(selector) = Selector::parse("img:not([width]):not([height])") {
        count = doc.select(&selector).count();
    }

    count
}

/// Check if LCP image has fetchpriority
pub fn check_lcp_optimization(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    
    // First image is likely LCP
    if let Ok(selector) = Selector::parse("img") {
        if let Some(first_img) = doc.select(&selector).next() {
            let attrs = first_img.value();
            
            // Check if it has fetchpriority="high"
            if attrs.attr("fetchpriority").is_none() {
                if let Some(src) = attrs.attr("src") {
                    return Some(format!(
                        "Add fetchpriority=\"high\" to LCP image: {}",
                        src
                    ));
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_images() {
        let html = r#"
            <img src="test.jpg">
            <img src="other.png" width="100" height="100">
            <img src="lazy.webp" loading="lazy">
        "#;
        
        let result = analyze_images(html);
        assert_eq!(result.webp_candidates, 2);
        assert_eq!(result.missing_dimensions, 2);
    }
}
