//! HTML/CSS/JS Optimizer

use scraper::{Html, Selector};

use crate::error::AppError;
use crate::handlers::OptimizeOptions;
use crate::css_optimizer::{CssOptimizer, minify_css};
use crate::seo_optimizer::{SeoOptimizer, add_alt_tags};

pub struct OptimizeResult {
    pub html: String,
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f64,
    pub optimizations: Vec<String>,
}

/// Main optimization function
pub fn optimize_html(html: &str, url: &str, options: &OptimizeOptions) -> Result<OptimizeResult, AppError> {
    let original_size = html.len();
    let mut optimized = html.to_string();
    let mut optimizations = Vec::new();

    tracing::debug!("Options: minify_css={}, minify_html={}, defer_js={}, lazy_images={}", 
        options.minify_css, options.minify_html, options.defer_js, options.lazy_images);

    // 1. Aggressive CSS tree-shaking FIRST (before HTML minification)
    if options.minify_css {
        let css_result = optimize_and_treeshake_css(&mut optimized);
        if css_result.0 > 0 {
            optimizations.push(format!("{} style blocks optimized ({}% reduction)", css_result.0, css_result.1));
        }
    }

    // 2. Minify HTML (after CSS is processed)
    if options.minify_html {
        optimized = minify_html(&optimized);
        optimizations.push("HTML minified".to_string());
    }

    // 3. Add lazy loading to images
    if options.lazy_images {
        let count = add_lazy_loading(&mut optimized);
        if count > 0 {
            optimizations.push(format!("{} images lazy-loaded", count));
        }
    }

    // 4. Defer JavaScript
    if options.defer_js {
        let count = defer_scripts(&mut optimized);
        if count > 0 {
            optimizations.push(format!("{} scripts deferred", count));
        }
    }

    // 5. Add image dimensions hint
    let dims_count = count_images_without_dimensions(&optimized);
    if dims_count > 0 {
        optimizations.push(format!("{} images need dimensions", dims_count));
    }

    // 6. Add preconnect hints for external resources
    let preconnects = add_preconnect_hints(&mut optimized);
    if preconnects > 0 {
        optimizations.push(format!("{} preconnect hints added", preconnects));
    }

    // 7. SEO Optimizations
    let seo_optimizer = SeoOptimizer::new();
    let seo_result = seo_optimizer.optimize(&mut optimized, url);
    for change in seo_result.changes {
        optimizations.push(format!("SEO: {}", change));
    }

    // 8. Schema.org structured data
    let schemas_added = crate::schema_generator::inject_schema(&mut optimized, url);
    if schemas_added > 0 {
        optimizations.push(format!("{} Schema.org types added", schemas_added));
    }

    // 9. Image optimization analysis
    let image_result = crate::image_optimizer::analyze_images(&optimized);
    for opt in image_result.optimizations {
        optimizations.push(format!("Image: {}", opt));
    }
    if let Some(lcp_hint) = crate::image_optimizer::check_lcp_optimization(&optimized) {
        optimizations.push(format!("LCP: {}", lcp_hint));
    }

    // 10. CDN Image URL Rewriting - DISABLED (using Rust WebP conversion instead)
    // The WebP conversion in handlers.rs will download images, convert them,
    // and return base64 data for WordPress to save locally. No CDN needed.
    // To re-enable CDN rewrite, uncomment the following:
    // let cdn_config = crate::image_optimizer::CdnConfig::default();
    // let cdn_count = crate::image_optimizer::rewrite_images_for_cdn(&mut optimized, url, &cdn_config);
    // if cdn_count > 0 {
    //     optimizations.push(format!("{} images rewritten for CDN ({})", cdn_count, cdn_config.provider));
    // }

    let optimized_size = optimized.len();
    let reduction = if original_size > 0 {
        (1.0 - (optimized_size as f64 / original_size as f64)) * 100.0
    } else {
        0.0
    };

    tracing::debug!(
        "Final stats: original={} optimized={} reduction={:.1}% optimizations={}",
        original_size, optimized_size, reduction, optimizations.len()
    );

    Ok(OptimizeResult {
        html: optimized,
        original_size,
        optimized_size,
        reduction_percent: (reduction * 10.0).round() / 10.0,
        optimizations,
    })
}

/// Optimize inline CSS with aggressive tree-shaking
fn optimize_and_treeshake_css(html: &mut String) -> (usize, i32) {
    tracing::debug!("CSS tree-shake: Starting, HTML len = {}", html.len());
    
    // First, extract all selectors used in HTML
    let mut css_optimizer = CssOptimizer::new();
    css_optimizer.extract_used_selectors(html);

    let mut count = 0;
    let mut total_reduction: i32 = 0;
    let mut result = String::with_capacity(html.len());
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();

    while i < len {
        // Look for <style
        if i + 5 < len {
            let tag: String = chars[i..i+6].iter().collect();
            if tag.to_lowercase() == "<style" {
                tracing::debug!("CSS tree-shake: Found <style at position {}", i);
                // Find end of opening tag
                let start = i;
                while i < len && chars[i] != '>' {
                    i += 1;
                }
                if i < len {
                    i += 1; // past >
                }
                
                let open_tag: String = chars[start..i].iter().collect();
                result.push_str(&open_tag);
                
                // Find </style>
                let css_start = i;
                while i + 7 < len {
                    let closing: String = chars[i..i+8].iter().collect();
                    if closing.to_lowercase() == "</style>" {
                        break;
                    }
                    i += 1;
                }
                
                let css_content: String = chars[css_start..i].iter().collect();
                let original_len = css_content.len();
                
                // Skip tree-shaking for very large CSS blocks (>100KB) to prevent hangs
                if original_len > 100_000 {
                    tracing::warn!("Skipping CSS tree-shake for large block: {} bytes", original_len);
                    result.push_str(&css_content);
                    result.push_str("</style>");
                    i += 8;
                    continue;
                }
                
                // Tree-shake the CSS - remove unused rules
                match css_optimizer.remove_unused_css(&css_content) {
                    Ok(optimized) => {
                        let new_len = optimized.len();
                        if original_len > 0 {
                            let reduction = ((original_len.saturating_sub(new_len)) as f64 / original_len as f64 * 100.0) as i32;
                            total_reduction += reduction;
                        }
                        result.push_str(&optimized);
                        count += 1;
                        tracing::debug!(
                            "CSS tree-shake: {} -> {} bytes ({}% reduction)",
                            original_len, new_len, 
                            if original_len > 0 { (original_len - new_len) * 100 / original_len } else { 0 }
                        );
                    }
                    Err(e) => {
                        // Keep original on error
                        tracing::warn!("CSS optimization failed: {}", e);
                        result.push_str(&css_content);
                    }
                }
                
                // Add closing tag
                result.push_str("</style>");
                i += 8; // skip </style>
                continue;
            }
        }
        
        result.push(chars[i]);
        i += 1;
    }

    let avg_reduction = if count > 0 { total_reduction / count as i32 } else { 0 };
    *html = result;
    (count, avg_reduction)
}

/// Add preconnect hints for common external resources
fn add_preconnect_hints(html: &mut String) -> usize {
    let mut hints_added = 0;
    let mut preconnect_domains: Vec<&str> = Vec::new();

    // Check for common external resources
    if html.contains("fonts.googleapis.com") && !html.contains("preconnect") {
        preconnect_domains.push("https://fonts.googleapis.com");
        preconnect_domains.push("https://fonts.gstatic.com");
    }
    if html.contains("googletagmanager.com") {
        preconnect_domains.push("https://www.googletagmanager.com");
    }
    if html.contains("google-analytics.com") {
        preconnect_domains.push("https://www.google-analytics.com");
    }

    if preconnect_domains.is_empty() {
        return 0;
    }

    // Build preconnect links
    let mut preconnect_html = String::new();
    for domain in &preconnect_domains {
        preconnect_html.push_str(&format!(
            "<link rel=\"preconnect\" href=\"{}\" crossorigin>",
            domain
        ));
        hints_added += 1;
    }

    // Insert after <head>
    if let Some(pos) = html.to_lowercase().find("<head>") {
        let insert_pos = pos + 6;
        html.insert_str(insert_pos, &preconnect_html);
    }

    hints_added
}

/// Minify HTML by removing unnecessary whitespace and comments
fn minify_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_pre = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut in_comment = false;
    let mut last_was_space = false;

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Check for comment start
        if i + 3 < len && chars[i..i+4].iter().collect::<String>() == "<!--" {
            in_comment = true;
            i += 4;
            continue;
        }

        // Check for comment end
        if in_comment {
            if i + 2 < len && chars[i..i+3].iter().collect::<String>() == "-->" {
                in_comment = false;
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }

        // Check for tag starts
        let remaining: String = chars[i..].iter().take(10).collect();
        let remaining_lower = remaining.to_lowercase();

        if remaining_lower.starts_with("<pre") {
            in_pre = true;
        } else if remaining_lower.starts_with("</pre") {
            in_pre = false;
        } else if remaining_lower.starts_with("<script") {
            in_script = true;
        } else if remaining_lower.starts_with("</script") {
            in_script = false;
        } else if remaining_lower.starts_with("<style") {
            in_style = true;
        } else if remaining_lower.starts_with("</style") {
            in_style = false;
        }

        let c = chars[i];

        // Preserve whitespace in pre, script, style
        if in_pre || in_script || in_style {
            result.push(c);
            last_was_space = false;
        } else if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }

        i += 1;
    }

    result
}

/// Add lazy loading to images below the fold
fn add_lazy_loading(html: &mut String) -> usize {
    let mut count = 0;
    
    // Simple regex-like replacement for img tags
    let mut result = String::with_capacity(html.len() + 1000);
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();

    while i < len {
        // Look for <img
        if i + 3 < len {
            let tag: String = chars[i..i+4].iter().collect();
            if tag.to_lowercase() == "<img" {
                // Find the end of the tag
                let start = i;
                while i < len && chars[i] != '>' {
                    i += 1;
                }
                if i < len {
                    i += 1; // include >
                }
                
                let img_tag: String = chars[start..i].iter().collect();
                
                // Skip if already has loading attribute or is likely LCP image
                if !img_tag.contains("loading=") && !img_tag.contains("fetchpriority=") {
                    // Add loading="lazy"
                    let new_tag = img_tag.replacen("<img", "<img loading=\"lazy\"", 1);
                    result.push_str(&new_tag);
                    count += 1;
                    continue;
                } else {
                    result.push_str(&img_tag);
                    continue;
                }
            }
        }
        
        result.push(chars[i]);
        i += 1;
    }

    *html = result;
    count
}

/// Defer non-critical scripts
fn defer_scripts(html: &mut String) -> usize {
    let mut count = 0;
    
    let mut result = String::with_capacity(html.len() + 500);
    let mut i = 0;
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();

    while i < len {
        // Look for <script
        if i + 6 < len {
            let tag: String = chars[i..i+7].iter().collect();
            if tag.to_lowercase() == "<script" {
                let start = i;
                while i < len && chars[i] != '>' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                
                let script_tag: String = chars[start..i].iter().collect();
                
                // Skip if already has defer/async or is inline
                let lower = script_tag.to_lowercase();
                if !lower.contains("defer") && !lower.contains("async") && lower.contains("src=") {
                    // Add defer
                    let new_tag = script_tag.replacen("<script", "<script defer", 1);
                    result.push_str(&new_tag);
                    count += 1;
                    continue;
                } else {
                    result.push_str(&script_tag);
                    continue;
                }
            }
        }
        
        result.push(chars[i]);
        i += 1;
    }

    *html = result;
    count
}

/// Count images without width/height (causes CLS)
fn count_images_without_dimensions(html: &str) -> usize {
    // For MVP, we'll just count images without dimensions
    // Full implementation would fetch image dimensions
    let doc = Html::parse_document(html);
    let selector = Selector::parse("img:not([width]):not([height])").unwrap_or_else(|_| {
        Selector::parse("img").unwrap()
    });
    
    doc.select(&selector).count()
}
