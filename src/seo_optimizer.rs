//! SEO Optimizer Module
//! Handles alt tags, meta descriptions, Open Graph, Twitter Cards, and Schema.org

use scraper::{Html, Selector};
use std::collections::HashMap;

/// SEO analysis result
pub struct SeoResult {
    pub html: String,
    pub changes: Vec<String>,
    pub warnings: Vec<String>,
    pub score: u8, // 0-100
}

/// SEO Optimizer
pub struct SeoOptimizer {
    /// Site name for OG tags
    pub site_name: String,
    /// Default OG image
    pub default_og_image: Option<String>,
}

impl SeoOptimizer {
    pub fn new() -> Self {
        Self {
            site_name: String::new(),
            default_og_image: None,
        }
    }

    /// Run all SEO optimizations
    pub fn optimize(&self, html: &str, url: &str) -> SeoResult {
        let mut optimized = html.to_string();
        let mut changes = Vec::new();
        let mut warnings = Vec::new();

        // 1. Fix images without alt tags
        let alt_count = add_alt_tags(&mut optimized);
        if alt_count > 0 {
            changes.push(format!("{} alt tags added", alt_count));
        }

        // 2. Check/add meta description
        let meta_result = ensure_meta_description(&mut optimized);
        match meta_result {
            MetaResult::Added => changes.push("Meta description added".to_string()),
            MetaResult::TooShort => warnings.push("Meta description too short (<120 chars)".to_string()),
            MetaResult::TooLong => warnings.push("Meta description too long (>160 chars)".to_string()),
            MetaResult::Exists => {}
        }

        // 3. Add Open Graph tags
        let og_count = add_open_graph_tags(&mut optimized, url, &self.site_name);
        if og_count > 0 {
            changes.push(format!("{} Open Graph tags added", og_count));
        }

        // 4. Add Twitter Card tags
        let twitter_count = add_twitter_card_tags(&mut optimized);
        if twitter_count > 0 {
            changes.push(format!("{} Twitter Card tags added", twitter_count));
        }

        // 5. Add canonical URL
        let canonical_added = add_canonical_url(&mut optimized, url);
        if canonical_added {
            changes.push("Canonical URL added".to_string());
        }

        // 6. Fix external links (add rel="noopener")
        let links_fixed = fix_external_links(&mut optimized);
        if links_fixed > 0 {
            changes.push(format!("{} external links secured", links_fixed));
        }

        // Calculate SEO score (simplified)
        let score = calculate_seo_score(&optimized);

        SeoResult {
            html: optimized,
            changes,
            warnings,
            score,
        }
    }
}

enum MetaResult {
    Added,
    Exists,
    TooShort,
    TooLong,
}

/// Add alt tags to images that don't have them
pub fn add_alt_tags(html: &mut String) -> usize {
    let mut count = 0;
    let mut result = String::with_capacity(html.len() + 2000);
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 3 < len {
            let tag: String = chars[i..i+4].iter().collect();
            if tag.to_lowercase() == "<img" {
                let start = i;
                while i < len && chars[i] != '>' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }

                let img_tag: String = chars[start..i].iter().collect();
                
                // Check if alt attribute exists
                if !img_tag.to_lowercase().contains("alt=") {
                    // Extract filename from src for alt text
                    let alt_text = extract_alt_from_src(&img_tag);
                    let new_tag = img_tag.replacen("<img", &format!("<img alt=\"{}\"", alt_text), 1);
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

/// Extract a reasonable alt text from image src
fn extract_alt_from_src(img_tag: &str) -> String {
    // Try to find src attribute
    let lower = img_tag.to_lowercase();
    if let Some(src_start) = lower.find("src=") {
        let quote_start = src_start + 4;
        let remaining = &img_tag[quote_start..];
        
        // Find the quote character used
        let quote_char = remaining.chars().next().unwrap_or('"');
        if quote_char == '"' || quote_char == '\'' {
            let src_content = &remaining[1..];
            if let Some(end) = src_content.find(quote_char) {
                let src = &src_content[..end];
                
                // Extract filename without extension
                if let Some(filename) = src.split('/').last() {
                    let name = filename
                        .split('.')
                        .next()
                        .unwrap_or("image")
                        .replace('-', " ")
                        .replace('_', " ");
                    
                    // Capitalize first letter
                    let mut chars: Vec<char> = name.chars().collect();
                    if !chars.is_empty() {
                        chars[0] = chars[0].to_uppercase().next().unwrap_or(chars[0]);
                    }
                    return chars.into_iter().collect();
                }
            }
        }
    }
    
    "Image".to_string()
}

/// Ensure meta description exists
fn ensure_meta_description(html: &mut String) -> MetaResult {
    let lower = html.to_lowercase();
    
    // Check if meta description exists
    if lower.contains("name=\"description\"") || lower.contains("name='description'") {
        // Check length
        if let Some(start) = lower.find("name=\"description\"") {
            let remaining = &html[start..];
            if let Some(content_start) = remaining.to_lowercase().find("content=") {
                let after_content = &remaining[content_start + 8..];
                let quote_char = after_content.chars().next().unwrap_or('"');
                if quote_char == '"' || quote_char == '\'' {
                    let content = &after_content[1..];
                    if let Some(end) = content.find(quote_char) {
                        let desc = &content[..end];
                        if desc.len() < 120 {
                            return MetaResult::TooShort;
                        } else if desc.len() > 160 {
                            return MetaResult::TooLong;
                        }
                    }
                }
            }
        }
        return MetaResult::Exists;
    }

    // Generate from content if missing
    let description = generate_description_from_content(html);
    
    // Insert after <head>
    if let Some(pos) = lower.find("<head>") {
        let insert_pos = pos + 6;
        let meta_tag = format!("\n<meta name=\"description\" content=\"{}\">\n", description);
        html.insert_str(insert_pos, &meta_tag);
        return MetaResult::Added;
    }

    MetaResult::Exists
}

/// Generate a description from page content
fn generate_description_from_content(html: &str) -> String {
    let doc = Html::parse_document(html);
    
    // Try to get first paragraph
    if let Ok(selector) = Selector::parse("p") {
        for element in doc.select(&selector) {
            let text: String = element.text().collect::<Vec<_>>().join(" ");
            let clean = text.trim();
            if clean.len() > 50 {
                // Truncate to ~155 chars at word boundary
                let truncated: String = clean.chars().take(155).collect();
                if let Some(last_space) = truncated.rfind(' ') {
                    return format!("{}...", &truncated[..last_space]);
                }
                return format!("{}...", truncated);
            }
        }
    }

    // Fallback: use title
    if let Ok(selector) = Selector::parse("title") {
        for element in doc.select(&selector) {
            let text: String = element.text().collect();
            return text.trim().to_string();
        }
    }

    "".to_string()
}

/// Add Open Graph tags
fn add_open_graph_tags(html: &mut String, url: &str, site_name: &str) -> usize {
    let lower = html.to_lowercase();
    let mut count = 0;
    let mut og_tags = String::new();

    // og:url
    if !lower.contains("og:url") {
        og_tags.push_str(&format!("<meta property=\"og:url\" content=\"{}\">\n", url));
        count += 1;
    }

    // og:type
    if !lower.contains("og:type") {
        og_tags.push_str("<meta property=\"og:type\" content=\"website\">\n");
        count += 1;
    }

    // og:title (from <title>)
    if !lower.contains("og:title") {
        let doc = Html::parse_document(html);
        if let Ok(selector) = Selector::parse("title") {
            if let Some(element) = doc.select(&selector).next() {
                let title: String = element.text().collect();
                og_tags.push_str(&format!("<meta property=\"og:title\" content=\"{}\">\n", title.trim()));
                count += 1;
            }
        }
    }

    // og:description (from meta description)
    if !lower.contains("og:description") {
        let doc = Html::parse_document(html);
        if let Ok(selector) = Selector::parse("meta[name=\"description\"]") {
            if let Some(element) = doc.select(&selector).next() {
                if let Some(content) = element.value().attr("content") {
                    og_tags.push_str(&format!("<meta property=\"og:description\" content=\"{}\">\n", content));
                    count += 1;
                }
            }
        }
    }

    // og:image (from first image)
    if !lower.contains("og:image") {
        let doc = Html::parse_document(html);
        if let Ok(selector) = Selector::parse("img[src]") {
            if let Some(element) = doc.select(&selector).next() {
                if let Some(src) = element.value().attr("src") {
                    // Make absolute URL if relative
                    let img_url = if src.starts_with("http") {
                        src.to_string()
                    } else if let Some(base) = url.split('/').take(3).collect::<Vec<_>>().join("/").into() {
                        format!("{}{}", base, src)
                    } else {
                        src.to_string()
                    };
                    og_tags.push_str(&format!("<meta property=\"og:image\" content=\"{}\">\n", img_url));
                    count += 1;
                }
            }
        }
    }

    // og:site_name
    if !lower.contains("og:site_name") && !site_name.is_empty() {
        og_tags.push_str(&format!("<meta property=\"og:site_name\" content=\"{}\">\n", site_name));
        count += 1;
    }

    // Insert OG tags
    if count > 0 {
        if let Some(pos) = lower.find("</head>") {
            html.insert_str(pos, &og_tags);
        }
    }

    count
}

/// Add Twitter Card tags
fn add_twitter_card_tags(html: &mut String) -> usize {
    let lower = html.to_lowercase();
    let mut count = 0;
    let mut twitter_tags = String::new();

    // twitter:card
    if !lower.contains("twitter:card") {
        twitter_tags.push_str("<meta name=\"twitter:card\" content=\"summary_large_image\">\n");
        count += 1;
    }

    // twitter:title (inherit from og:title if available)
    if !lower.contains("twitter:title") && lower.contains("og:title") {
        // Twitter uses OG fallback, so this is optional
    }

    // Insert Twitter tags
    if count > 0 {
        if let Some(pos) = lower.find("</head>") {
            html.insert_str(pos, &twitter_tags);
        }
    }

    count
}

/// Add canonical URL if missing
fn add_canonical_url(html: &mut String, url: &str) -> bool {
    let lower = html.to_lowercase();
    
    if lower.contains("rel=\"canonical\"") || lower.contains("rel='canonical'") {
        return false;
    }

    let canonical = format!("<link rel=\"canonical\" href=\"{}\">\n", url);
    
    if let Some(pos) = lower.find("</head>") {
        html.insert_str(pos, &canonical);
        return true;
    }

    false
}

/// Fix external links to add rel="noopener noreferrer"
fn fix_external_links(html: &mut String) -> usize {
    let mut count = 0;
    let mut result = String::with_capacity(html.len() + 1000);
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len {
            let tag: String = chars[i..i+2].iter().collect();
            if tag.to_lowercase() == "<a" {
                let start = i;
                while i < len && chars[i] != '>' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }

                let a_tag: String = chars[start..i].iter().collect();
                let lower = a_tag.to_lowercase();
                
                // Check if external link (has http and target="_blank")
                let is_external = lower.contains("http") && 
                    (lower.contains("target=\"_blank\"") || lower.contains("target='_blank'"));
                
                // Check if already has noopener
                let has_noopener = lower.contains("noopener");
                
                if is_external && !has_noopener {
                    // Add rel="noopener noreferrer"
                    let new_tag = if lower.contains("rel=") {
                        // Append to existing rel
                        a_tag.replace("rel=\"", "rel=\"noopener noreferrer ")
                             .replace("rel='", "rel='noopener noreferrer ")
                    } else {
                        // Add new rel attribute
                        a_tag.replacen("<a", "<a rel=\"noopener noreferrer\"", 1)
                    };
                    result.push_str(&new_tag);
                    count += 1;
                    continue;
                } else {
                    result.push_str(&a_tag);
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

/// Calculate a simple SEO score
fn calculate_seo_score(html: &str) -> u8 {
    let lower = html.to_lowercase();
    let mut score: u8 = 50; // Base score

    // Title exists (+10)
    if lower.contains("<title>") && lower.contains("</title>") {
        score = score.saturating_add(10);
    }

    // Meta description (+10)
    if lower.contains("name=\"description\"") {
        score = score.saturating_add(10);
    }

    // H1 tag exists (+10)
    if lower.contains("<h1") && lower.contains("</h1>") {
        score = score.saturating_add(10);
    }

    // Open Graph tags (+10)
    if lower.contains("og:title") && lower.contains("og:description") {
        score = score.saturating_add(10);
    }

    // Canonical URL (+5)
    if lower.contains("rel=\"canonical\"") {
        score = score.saturating_add(5);
    }

    // All images have alt (+5)
    let doc = Html::parse_document(html);
    if let Ok(selector) = Selector::parse("img:not([alt])") {
        if doc.select(&selector).count() == 0 {
            score = score.saturating_add(5);
        }
    }

    score.min(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_alt_from_src() {
        let img = r#"<img src="/images/hero-banner.jpg">"#;
        let alt = extract_alt_from_src(img);
        assert_eq!(alt, "Hero banner");
    }

    #[test]
    fn test_add_alt_tags() {
        let mut html = r#"<img src="test.jpg"><img src="other.png" alt="exists">"#.to_string();
        let count = add_alt_tags(&mut html);
        assert_eq!(count, 1);
        assert!(html.contains("alt=\"Test\""));
    }
}
