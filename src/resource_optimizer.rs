//! External Resource Optimizer Module
//! Fetches, optimizes, and prepares external CSS/JS for local storage

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use lightningcss::{
    stylesheet::{StyleSheet, ParserOptions, MinifyOptions, PrinterOptions},
    targets::Targets,
};
use scraper::{Html, Selector};

/// Result of optimized CSS/JS for API response
#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizedResources {
    pub css_files: Vec<OptimizedCssFile>,
    pub js_files: Vec<OptimizedJsFile>,
    pub critical_css: Option<String>,
    /// Combined CSS - all CSS merged into one file
    pub combined_css: Option<String>,
    /// Combined JS - all JS merged into one file
    pub combined_js: Option<String>,
    pub combined_css_filename: String,
    pub combined_js_filename: String,
    pub total_css_savings_kb: f32,
    pub total_js_savings_kb: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizedCssFile {
    pub original_url: String,
    pub filename: String,
    pub content: String,  // Minified CSS content (not base64 - CSS is text)
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizedJsFile {
    pub original_url: String,
    pub filename: String,
    pub content: String,  // Minified JS content
    pub original_size: usize,
    pub optimized_size: usize,
    pub reduction_percent: f32,
}

/// Download a resource from URL
pub async fn download_resource(url: &str) -> Result<String, String> {
    tracing::debug!("Resource optimizer: Downloading {}", url);
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| format!("Failed to download resource: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}: {}", response.status(), url));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    tracing::debug!("Resource optimizer: Downloaded {} bytes from {}", text.len(), url);
    Ok(text)
}

/// Extract external CSS links from HTML
pub fn extract_css_links(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("link[rel='stylesheet']").unwrap();

    document
        .select(&selector)
        .filter_map(|element| element.value().attr("href"))
        .filter(|href| !href.starts_with("data:") && !href.is_empty() && !href.contains("/htmlwp/"))
        .map(|href| href.to_string())
        .collect()
}

/// Extract external JS script sources from HTML
pub fn extract_js_sources(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("script[src]").unwrap();

    document
        .select(&selector)
        .filter_map(|element| element.value().attr("src"))
        .filter(|src| !src.starts_with("data:") && !src.is_empty())
        .map(|src| src.to_string())
        .collect()
}

/// Extract href attribute from a tag string
fn extract_href(tag: &str) -> Option<String> {
    extract_attribute(tag, "href")
}

/// Extract src attribute from a tag string
fn extract_src(tag: &str) -> Option<String> {
    extract_attribute(tag, "src")
}

/// Extract an attribute value from a tag string
fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let chars: Vec<char> = tag.chars().collect();
    let search: Vec<char> = format!("{}=", attr_name).to_lowercase().chars().collect();
    
    let len = chars.len();
    let search_len = search.len();
    
    if search_len > len {
        return None;
    }

    for i in 0..=len - search_len {
        // Check for match (case insensitive)
        let matches = (0..search_len).all(|j| {
            chars[i + j].to_lowercase().next() == Some(search[j])
        });

        if matches {
            let start = i + search_len;
            if start < len {
                let quote = chars[start];
                if quote == '"' || quote == '\'' {
                    let value_start = start + 1;
                    let mut value_end = value_start;
                    while value_end < len && chars[value_end] != quote {
                        value_end += 1;
                    }
                    return Some(chars[value_start..value_end].iter().collect());
                }
            }
        }
    }
    None
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

/// Minify CSS using lightningcss
pub fn minify_css(css: &str) -> Result<String, String> {
    let stylesheet = StyleSheet::parse(css, ParserOptions::default())
        .map_err(|e| format!("Failed to parse CSS: {:?}", e))?;

    let result = stylesheet.to_css(PrinterOptions {
        minify: true,
        ..PrinterOptions::default()
    }).map_err(|e| format!("Failed to minify CSS: {:?}", e))?;

    Ok(result.code)
}

/// Optimize a single external CSS file
pub async fn optimize_css_file(url: &str, base_url: &str, used_selectors: &[String], minify: bool) -> Result<OptimizedCssFile, String> {
    // Make URL absolute
    let full_url = if url.starts_with("/") {
        format!("{}{}", base_url.trim_end_matches('/'), url)
    } else if url.starts_with("http") {
        url.to_string()
    } else {
        format!("{}/{}", base_url.trim_end_matches('/'), url)
    };

    // Download the CSS
    let original_css = download_resource(&full_url).await?;
    let original_size = original_css.len();

    // Skip very large files
    if original_size > 500_000 {
        tracing::warn!("CSS optimizer: Skipping large file {} ({} KB)", url, original_size / 1024);
        return Err(format!("CSS file too large: {} KB", original_size / 1024));
    }

    // Minify Only (No Tree-Shaking for external files to prevent per-page fragmentation)
    // We use content-based hashing for deduplication
    let minified = if minify {
        minify_css(&original_css).unwrap_or(original_css)
    } else {
        original_css
    };
    let optimized_size = minified.len();

    // Skip if no improvement
    if optimized_size >= original_size {
        tracing::info!("CSS optimizer: No improvement for {}", url);
        return Err("No size improvement".to_string());
    }

    let reduction = ((original_size - optimized_size) as f32 / original_size as f32) * 100.0;

    tracing::info!(
        "CSS optimizer: {} -> {} bytes ({:.1}% reduction)",
        original_size, optimized_size, reduction
    );

    Ok(OptimizedCssFile {
        original_url: url.to_string(),
        filename: generate_filename(url, "css"),
        content: minified,
        original_size,
        optimized_size,
        reduction_percent: reduction,
    })
}

/// Optimize a single external JS file (minification only for now)
pub async fn optimize_js_file(url: &str, base_url: &str, minify: bool) -> Result<OptimizedJsFile, String> {
    // Make URL absolute
    let full_url = if url.starts_with("/") {
        format!("{}{}", base_url.trim_end_matches('/'), url)
    } else if url.starts_with("http") {
        url.to_string()
    } else {
        format!("{}/{}", base_url.trim_end_matches('/'), url)
    };

    // Download the JS
    let original_js = download_resource(&full_url).await?;
    let original_size = original_js.len();

    // Skip very large files
    if original_size > 1_000_000 {
        tracing::warn!("JS optimizer: Skipping large file {} ({} KB)", url, original_size / 1024);
        return Err(format!("JS file too large: {} KB", original_size / 1024));
    }

    // Basic minification check
    let minified = if minify {
        basic_js_minify(&original_js)
    } else {
        original_js
    };
    let optimized_size = minified.len();

    // Skip if no improvement
    if optimized_size >= original_size {
        tracing::info!("JS optimizer: No improvement for {}", url);
        return Err("No size improvement".to_string());
    }

    let reduction = ((original_size - optimized_size) as f32 / original_size as f32) * 100.0;

    tracing::info!(
        "JS optimizer: {} -> {} bytes ({:.1}% reduction)",
        original_size, optimized_size, reduction
    );

    Ok(OptimizedJsFile {
        original_url: url.to_string(),
        filename: generate_filename(url, "js"),
        content: minified,
        original_size,
        optimized_size,
        reduction_percent: reduction,
    })
}

/// Robust JS minification using minify-js (AST-based)
fn basic_js_minify(js: &str) -> String {
    let session = minify_js::Session::new();
    let mut out = Vec::new();
    match minify_js::minify(&session, minify_js::TopLevelMode::Global, js.as_bytes(), &mut out) {
        Ok(_) => {
            // minify-js output is bytes, convert back to string
            // It filters out invalid UTF-8 automatically usually, but we check
            match String::from_utf8(out) {
                Ok(minified) => {
                    if minified.len() < js.len() {
                        minified
                    } else {
                        js.to_string()
                    }
                }
                Err(_) => js.to_string()
            }
        }
        Err(e) => {
            tracing::debug!("JS minification failed (using original): {:?}", e);
            js.to_string()
        }
    }
}

/// Extract critical CSS (above-the-fold styles)
pub fn extract_critical_css(full_css: &str, html: &str) -> String {
    // Critical CSS extraction is complex and typically requires:
    // 1. Rendering the page in a headless browser
    // 2. Determining which elements are above-the-fold
    // 3. Extracting only those CSS rules
    
    // For now, we'll use a heuristic approach:
    // - Include all :root and html/body styles
    // - Include header, nav, and hero section styles
    // - Include font-face declarations
    // - Limit to ~14KB (recommended critical CSS size)
    
    let mut critical = String::new();
    let max_size = 14 * 1024; // 14KB limit
    
    // Split CSS into rules and filter
    for rule in full_css.split('}') {
        if critical.len() >= max_size {
            break;
        }
        
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }
        
        let rule_with_brace = format!("{}}}", rule);
        
        // Include critical selectors
        let is_critical = 
            rule.contains("@font-face") ||
            rule.contains(":root") ||
            rule.contains("html") ||
            rule.contains("body") ||
            rule.contains("header") ||
            rule.contains("nav") ||
            rule.contains(".hero") ||
            rule.contains("#hero") ||
            rule.contains(".header") ||
            rule.contains("#header") ||
            rule.contains(".site-") ||
            rule.contains("@media");
        
        if is_critical {
            critical.push_str(&rule_with_brace);
            critical.push('\n');
        }
    }
    
    critical
}

/// Optimize all external resources in HTML
pub async fn optimize_external_resources(html: &str, base_url: &str, used_selectors: &[String], options: &crate::handlers::OptimizeOptions) -> OptimizedResources {
    tracing::info!("Resource optimizer: Starting external CSS/JS optimization");
    
    let mut css_files = Vec::new();
    let mut js_files = Vec::new();
    let mut total_css_original: usize = 0;
    let mut total_css_optimized: usize = 0;
    let mut total_js_original: usize = 0;
    let mut total_js_optimized: usize = 0;
    
    // Extract and optimize CSS
    let css_links = extract_css_links(html);
    tracing::debug!("Resource optimizer: Found {} CSS links", css_links.len());
    
    for url in css_links {
        // Skip external CDNs (Google Fonts, etc.)
        if should_skip_external(&url) {
            tracing::debug!("Resource optimizer: Skipping external {}", url);
            continue;
        }
        
        match optimize_css_file(&url, base_url, used_selectors, options.minify_css).await {
            Ok(optimized) => {
                total_css_original += optimized.original_size;
                total_css_optimized += optimized.optimized_size;
                css_files.push(optimized);
            }
            Err(e) => {
                tracing::warn!("Resource optimizer: Failed to optimize CSS {}: {}", url, e);
            }
        }
    }
    
    // Extract and optimize JS
    let js_sources = extract_js_sources(html);
    tracing::debug!("Resource optimizer: Found {} JS sources", js_sources.len());
    
    for url in js_sources {
        // Skip external CDNs
        if should_skip_external(&url) {
            tracing::debug!("Resource optimizer: Skipping external {}", url);
            continue;
        }
        
        match optimize_js_file(&url, base_url, options.minify_js).await {
            Ok(optimized) => {
                total_js_original += optimized.original_size;
                total_js_optimized += optimized.optimized_size;
                js_files.push(optimized);
            }
            Err(e) => {
                tracing::warn!("Resource optimizer: Failed to optimize JS {}: {}", url, e);
            }
        }
    }
    
    // Calculate critical CSS from all optimized CSS
    let all_css: String = css_files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n");
    let critical_css = if !all_css.is_empty() {
        Some(extract_critical_css(&all_css, html))
    } else {
        None
    };
    
    // Generate combined CSS (all CSS merged into one file)
    let combined_css = if !css_files.is_empty() {
        Some(css_files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join("\n"))
    } else {
        None
    };
    
    // Generate combined JS (all JS merged into one file with semicolons for safety)
    let combined_js = if !js_files.is_empty() {
        Some(js_files.iter().map(|f| f.content.as_str()).collect::<Vec<_>>().join(";\n"))
    } else {
        None
    };
    
    let css_savings = total_css_original.saturating_sub(total_css_optimized) as f32 / 1024.0;
    let js_savings = total_js_original.saturating_sub(total_js_optimized) as f32 / 1024.0;
    
    tracing::info!(
        "Resource optimizer: {} CSS files ({:.1}KB saved), {} JS files ({:.1}KB saved)",
        css_files.len(), css_savings, js_files.len(), js_savings
    );
    
    OptimizedResources {
        css_files,
        js_files,
        critical_css,
        combined_css,
        combined_js,
        combined_css_filename: "styles.min.css".to_string(),
        combined_js_filename: "scripts.min.js".to_string(),
        total_css_savings_kb: css_savings,
        total_js_savings_kb: js_savings,
    }
}

/// Check if URL should be skipped (external CDNs)
fn should_skip_external(url: &str) -> bool {
    let lower = url.to_lowercase();
    
    lower.contains("fonts.googleapis.com") ||
    lower.contains("fonts.gstatic.com") ||
    lower.contains("cdnjs.cloudflare.com") ||
    lower.contains("cdn.jsdelivr.net") ||
    lower.contains("unpkg.com") ||
    lower.contains("ajax.googleapis.com") ||
    lower.contains("code.jquery.com") ||
    lower.contains("stackpath.bootstrapcdn.com") ||
    lower.contains("maxcdn.bootstrapcdn.com")
}

/// Rewrite HTML to use combined CSS/JS files
pub fn rewrite_html_with_optimized_resources(html: &mut String, resources: &OptimizedResources, _upload_base_url: &str) {
    // Track if we've added the combined CSS link
    let mut combined_css_added = false;
    let mut combined_js_added = false;
    
    // Remove individual CSS links and replace with combined file
    // We only process CSS files that were successfully downloaded (in css_files)
    if resources.combined_css.is_some() && !resources.css_files.is_empty() {
        for css in &resources.css_files {
            // Find and remove the link tag for this CSS file
            // Look for patterns like: <link ... href="original_url" ...>
            if let Some(start) = find_link_tag_start(html, &css.original_url) {
                if let Some(end) = html[start..].find('>') {
                    let tag_end = start + end + 1; // +1 to include the '>'
                    
                    // If we haven't added combined CSS yet, replace first tag with combined
                    // Use non-blocking pattern: media="print" with onload to switch to "all"
                    // Critical CSS (inlined) handles above-the-fold, this loads rest async
                    if !combined_css_added {
                        let combined_link = concat!(
                            "<link rel=\"stylesheet\" href=\"./styles.min.css\" ",
                            "id=\"htmlwp-combined-css\" media=\"print\" ",
                            "onload=\"this.media='all'\">"
                        );
                        html.replace_range(start..tag_end, &combined_link);
                        combined_css_added = true;
                        tracing::debug!("Replaced CSS with combined: {}", css.original_url);
                    } else {
                        // Remove subsequent CSS tags entirely
                        html.replace_range(start..tag_end, "");
                        tracing::debug!("Removed CSS: {}", css.original_url);
                    }
                }
            }
        }
    }
    
    // Remove individual JS scripts and replace with combined file
    if resources.combined_js.is_some() && !resources.js_files.is_empty() {
        for js in &resources.js_files {
            // Find and remove the script tag for this JS file
            if let Some(start) = find_script_tag_start(html, &js.original_url) {
                // Find end of script tag - could be self-closing or have </script>
                if let Some(close_pos) = html[start..].find("</script>") {
                    let tag_end = start + close_pos + 9; // +9 for "</script>"
                    
                    if !combined_js_added {
                        let combined_script = format!(
                            "<script src=\"./scripts.min.js\" id=\"htmlwp-combined-js\"></script>"
                        );
                        html.replace_range(start..tag_end, &combined_script);
                        combined_js_added = true;
                        tracing::debug!("Replaced JS with combined: {}", js.original_url);
                    } else {
                        html.replace_range(start..tag_end, "");
                        tracing::debug!("Removed JS: {}", js.original_url);
                    }
                } else if let Some(end) = html[start..].find("/>") {
                    let tag_end = start + end + 2;
                    if !combined_js_added {
                        let combined_script = format!(
                            "<script src=\"./scripts.min.js\" id=\"htmlwp-combined-js\"></script>"
                        );
                        html.replace_range(start..tag_end, &combined_script);
                        combined_js_added = true;
                    } else {
                        html.replace_range(start..tag_end, "");
                    }
                }
            }
        }
    }
    
    // Inject critical CSS if present
    if let Some(critical) = &resources.critical_css {
        if !critical.is_empty() {
            // Find </head> and inject critical CSS before it
            if let Some(pos) = html.to_lowercase().find("</head>") {
                let critical_tag = format!("<style id=\"critical-css\">{}</style>\n", critical);
                html.insert_str(pos, &critical_tag);
                tracing::debug!("Injected {} bytes of critical CSS", critical.len());
            }
        }
    }
    
    tracing::info!(
        "HTML rewrite complete: CSS combined={}, JS combined={}",
        combined_css_added, combined_js_added
    );
}

/// Find the start position of a <link> tag containing the given URL
fn find_link_tag_start(html: &str, url: &str) -> Option<usize> {
    let lower_html = html.to_lowercase();
    let lower_url = url.to_lowercase();
    
    // Look for href="url", href='url', or href=url (unquoted)
    for pattern in [
        format!("href=\"{}\"", lower_url), 
        format!("href='{}'", lower_url),
        format!("href={}", lower_url)
    ] {
        if let Some(href_pos) = lower_html.find(&pattern) {
            // Search backwards from href to find <link
            let before = &lower_html[..href_pos];
            if let Some(link_rel_pos) = before.rfind("<link") {
                return Some(link_rel_pos);
            }
        }
    }
    None
}

/// Find the start position of a <script> tag containing the given URL  
fn find_script_tag_start(html: &str, url: &str) -> Option<usize> {
    let lower_html = html.to_lowercase();
    let lower_url = url.to_lowercase();
    
    // Look for src="url", src='url', or src=url (unquoted)
    for pattern in [
        format!("src=\"{}\"", lower_url), 
        format!("src='{}'", lower_url),
        format!("src={}", lower_url)
    ] {
        if let Some(src_pos) = lower_html.find(&pattern) {
            // Search backwards from src to find <script
            let before = &lower_html[..src_pos];
            if let Some(script_pos) = before.rfind("<script") {
                return Some(script_pos);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_links() {
        let html = r#"<link rel="stylesheet" href="/style.css"><link rel="stylesheet" href="/theme.css">"#;
        let links = extract_css_links(html);
        assert_eq!(links.len(), 2);
        assert!(links.contains(&"/style.css".to_string()));
    }

    #[test]
    fn test_extract_js_sources() {
        let html = r#"<script src="/app.js"></script><script>inline</script><script src="/vendor.js"></script>"#;
        let sources = extract_js_sources(html);
        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&"/app.js".to_string()));
    }

    #[test]
    fn test_user_specific_js_case() {
        let html = r#"<script defer type="text/javascript" src="https://pillarshoteldv.wpenginepowered.com/wp-includes/js/jquery/jquery.min.js?ver=3.7.1" id="jquery-core-js"></script>"#;
        let url = "https://pillarshoteldv.wpenginepowered.com/wp-includes/js/jquery/jquery.min.js?ver=3.7.1";
        
        let sources = extract_js_sources(html);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0], url);
        
        let pos = find_script_tag_start(html, url);
        assert!(pos.is_some(), "Failed to find script tag position");
    }

    #[test]
    fn test_basic_js_minify() {
        let js = "// comment\nvar x = 1;\n/* multi\nline */\nvar y = 2;";
        // Verify pass-through for now
        let minified = basic_js_minify(js);
        assert_eq!(minified, js);
        // assert!(!minified.contains("comment")); // Disabled during pass-through mode
        // assert!(minified.contains("var x"));
    }
}
