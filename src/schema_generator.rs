//! Schema.org Generator Module
//! Generates structured data for better SEO

use scraper::{Html, Selector};
use serde_json::json;

/// Schema.org result
pub struct SchemaResult {
    pub schemas_added: Vec<String>,
    pub json_ld: String,
}

/// Generate Schema.org JSON-LD for a page
pub fn generate_schema(html: &str, url: &str, page_type: &str) -> SchemaResult {
    let mut schemas = Vec::new();
    let mut json_ld_items: Vec<serde_json::Value> = Vec::new();

    // Extract page info
    let doc = Html::parse_document(html);
    let title = extract_title(&doc);
    let description = extract_description(&doc);
    let image = extract_first_image(&doc, url);

    match page_type {
        "article" | "post" => {
            let article_schema = generate_article_schema(&title, &description, url, &image);
            json_ld_items.push(article_schema);
            schemas.push("Article".to_string());
        }
        "product" => {
            let product_schema = generate_product_schema(&doc, url);
            if let Some(schema) = product_schema {
                json_ld_items.push(schema);
                schemas.push("Product".to_string());
            }
        }
        _ => {
            // Default: WebPage schema
            let webpage_schema = generate_webpage_schema(&title, &description, url);
            json_ld_items.push(webpage_schema);
            schemas.push("WebPage".to_string());
        }
    }

    // Add BreadcrumbList if we can detect breadcrumbs
    if let Some(breadcrumb) = generate_breadcrumb_schema(&doc, url) {
        json_ld_items.push(breadcrumb);
        schemas.push("BreadcrumbList".to_string());
    }

    // Combine all schemas
    let json_ld = if json_ld_items.len() == 1 {
        serde_json::to_string_pretty(&json_ld_items[0]).unwrap_or_default()
    } else {
        serde_json::to_string_pretty(&json_ld_items).unwrap_or_default()
    };

    SchemaResult {
        schemas_added: schemas,
        json_ld,
    }
}

/// Generate Article schema
fn generate_article_schema(title: &str, description: &str, url: &str, image: &str) -> serde_json::Value {
    json!({
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": title,
        "description": description,
        "url": url,
        "image": image,
        "author": {
            "@type": "Organization",
            "name": "Site Author"
        },
        "publisher": {
            "@type": "Organization",
            "name": "Site Publisher"
        }
    })
}

/// Generate WebPage schema
fn generate_webpage_schema(title: &str, description: &str, url: &str) -> serde_json::Value {
    json!({
        "@context": "https://schema.org",
        "@type": "WebPage",
        "name": title,
        "description": description,
        "url": url
    })
}

/// Generate Product schema (for WooCommerce)
fn generate_product_schema(doc: &Html, url: &str) -> Option<serde_json::Value> {
    // Look for WooCommerce product indicators
    let lower_html = doc.root_element().html().to_lowercase();
    
    if !lower_html.contains("woocommerce") && !lower_html.contains("product") {
        return None;
    }

    // Extract product info
    let name = extract_product_name(doc).unwrap_or_else(|| extract_title(doc));
    let price = extract_price(doc);
    let description = extract_description(doc);
    let image = extract_first_image(doc, url);

    Some(json!({
        "@context": "https://schema.org",
        "@type": "Product",
        "name": name,
        "description": description,
        "image": image,
        "url": url,
        "offers": {
            "@type": "Offer",
            "price": price,
            "priceCurrency": "USD",
            "availability": "https://schema.org/InStock"
        }
    }))
}

/// Generate BreadcrumbList schema
fn generate_breadcrumb_schema(doc: &Html, url: &str) -> Option<serde_json::Value> {
    // Look for breadcrumb elements
    let selectors = [
        ".breadcrumb",
        ".breadcrumbs", 
        "[class*='breadcrumb']",
        "nav[aria-label='breadcrumb']"
    ];

    for sel_str in selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            if doc.select(&selector).next().is_some() {
                // Found breadcrumbs, generate basic schema
                let path_parts: Vec<&str> = url.split('/').filter(|s| !s.is_empty()).collect();
                
                let items: Vec<serde_json::Value> = path_parts.iter().enumerate().map(|(i, part)| {
                    json!({
                        "@type": "ListItem",
                        "position": i + 1,
                        "name": part.replace('-', " ").replace('_', " "),
                        "item": format!("{}/{}", url.split('/').take(i + 4).collect::<Vec<_>>().join("/"), part)
                    })
                }).collect();

                if !items.is_empty() {
                    return Some(json!({
                        "@context": "https://schema.org",
                        "@type": "BreadcrumbList",
                        "itemListElement": items
                    }));
                }
            }
        }
    }

    None
}

/// Extract title from document
fn extract_title(doc: &Html) -> String {
    if let Ok(selector) = Selector::parse("title") {
        if let Some(element) = doc.select(&selector).next() {
            return element.text().collect::<String>().trim().to_string();
        }
    }
    String::new()
}

/// Extract meta description
fn extract_description(doc: &Html) -> String {
    if let Ok(selector) = Selector::parse("meta[name='description']") {
        if let Some(element) = doc.select(&selector).next() {
            if let Some(content) = element.value().attr("content") {
                return content.to_string();
            }
        }
    }
    String::new()
}

/// Extract first image URL
fn extract_first_image(doc: &Html, base_url: &str) -> String {
    if let Ok(selector) = Selector::parse("img[src]") {
        if let Some(element) = doc.select(&selector).next() {
            if let Some(src) = element.value().attr("src") {
                if src.starts_with("http") {
                    return src.to_string();
                } else {
                    // Make absolute
                    let base = base_url.split('/').take(3).collect::<Vec<_>>().join("/");
                    return format!("{}{}", base, src);
                }
            }
        }
    }
    String::new()
}

/// Extract product name (WooCommerce)
fn extract_product_name(doc: &Html) -> Option<String> {
    let selectors = [
        ".product_title",
        "h1.product-title",
        ".woocommerce-product-details__short-description h1",
    ];

    for sel_str in selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            if let Some(element) = doc.select(&selector).next() {
                let text: String = element.text().collect();
                if !text.is_empty() {
                    return Some(text.trim().to_string());
                }
            }
        }
    }
    None
}

/// Extract price from page
fn extract_price(doc: &Html) -> String {
    let selectors = [
        ".price .amount",
        ".product-price",
        "[class*='price']",
    ];

    for sel_str in selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            if let Some(element) = doc.select(&selector).next() {
                let text: String = element.text().collect();
                // Extract numeric value
                let price: String = text.chars()
                    .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
                    .collect();
                if !price.is_empty() {
                    return price.replace(',', "");
                }
            }
        }
    }
    "0".to_string()
}

/// Add Schema.org JSON-LD to HTML
pub fn inject_schema(html: &mut String, url: &str) -> usize {
    // Check if schema already exists
    if html.contains("application/ld+json") {
        return 0;
    }

    // Detect page type
    let page_type = detect_page_type(html);
    
    // Generate schema
    let result = generate_schema(html, url, &page_type);
    
    if result.json_ld.is_empty() {
        return 0;
    }

    // Inject before </head>
    let script = format!(
        "<script type=\"application/ld+json\">\n{}\n</script>\n",
        result.json_ld
    );

    if let Some(pos) = html.to_lowercase().find("</head>") {
        html.insert_str(pos, &script);
    }

    result.schemas_added.len()
}

/// Detect page type from HTML
fn detect_page_type(html: &str) -> String {
    let lower = html.to_lowercase();
    
    if lower.contains("woocommerce") && lower.contains("product") {
        return "product".to_string();
    }
    
    if lower.contains("hentry") || lower.contains("post-") || lower.contains("article") {
        return "article".to_string();
    }
    
    "page".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_webpage_schema() {
        let schema = generate_webpage_schema("Test Page", "A test description", "http://example.com");
        assert!(schema["@type"] == "WebPage");
        assert!(schema["name"] == "Test Page");
    }
}
