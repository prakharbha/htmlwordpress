//! CSS Optimizer Module
//! Handles Critical CSS extraction and Unused CSS removal

use scraper::{Html, Selector};
use std::collections::HashSet;
use lightningcss::stylesheet::{StyleSheet, ParserOptions, MinifyOptions, PrinterOptions};
use lightningcss::rules::CssRule;

/// CSS optimization result
pub struct CssResult {
    /// Critical CSS to inline in <head>
    pub critical_css: String,
    /// Non-critical CSS to defer
    pub deferred_css: String,
    /// Original total CSS size
    pub original_size: usize,
    /// Optimized total CSS size  
    pub optimized_size: usize,
    /// Percentage of CSS removed as unused
    pub unused_removed_percent: f64,
}

/// CSS Optimizer
pub struct CssOptimizer {
    /// Selectors used in HTML
    used_selectors: HashSet<String>,
    /// Class whitelist patterns (page builders, etc)
    whitelist_patterns: Vec<String>,
}

impl CssOptimizer {
    pub fn new() -> Self {
        Self {
            used_selectors: HashSet::new(),
            whitelist_patterns: vec![
                // WordPress core
                "wp-".to_string(),
                "admin-bar".to_string(),
                // Page builders
                "elementor-".to_string(),
                "e-".to_string(),
                "et_".to_string(),
                "et-".to_string(),
                "divi".to_string(),
                "fl-".to_string(),
                "vc_".to_string(),
                "wpb_".to_string(),
                "avia-".to_string(),
                "av-".to_string(),
                // Forms
                "wpcf7".to_string(),
                "gform".to_string(),
                "gfield".to_string(),
                // WooCommerce
                "woocommerce".to_string(),
                "wc-".to_string(),
                // Common dynamic classes
                "active".to_string(),
                "open".to_string(),
                "show".to_string(),
                "hidden".to_string(),
                "visible".to_string(),
                "hover".to_string(),
                "focus".to_string(),
                "selected".to_string(),
                "disabled".to_string(),
                "loading".to_string(),
            ],
        }
    }

    /// Extract all selectors used in HTML
    pub fn extract_used_selectors(&mut self, html: &str) {
        let document = Html::parse_document(html);
        
        // Get all classes
        if let Ok(selector) = Selector::parse("[class]") {
            for element in document.select(&selector) {
                if let Some(classes) = element.value().attr("class") {
                    for class in classes.split_whitespace() {
                        self.used_selectors.insert(format!(".{}", class));
                    }
                }
            }
        }

        // Get all IDs
        if let Ok(selector) = Selector::parse("[id]") {
            for element in document.select(&selector) {
                if let Some(id) = element.value().attr("id") {
                    self.used_selectors.insert(format!("#{}", id));
                }
            }
        }

        // Get all tag names
        for element in document.root_element().descendants() {
            if let Some(el) = element.value().as_element() {
                self.used_selectors.insert(el.name().to_string());
            }
        }
    }

    /// Static helper: Extract used selectors from HTML and return as Vec
    pub fn extract_used_selectors_static(html: &str) -> Vec<String> {
        let mut optimizer = Self::new();
        optimizer.extract_used_selectors(html);
        optimizer.used_selectors.into_iter().collect()
    }

    /// Create optimizer with pre-computed selectors
    pub fn with_selectors(selectors: &[String]) -> Self {
        let mut optimizer = Self::new();
        for selector in selectors {
            optimizer.used_selectors.insert(selector.clone());
        }
        optimizer
    }

    /// Check if a selector is used or whitelisted
    fn is_selector_used(&self, selector: &str) -> bool {
        let selector_trimmed = selector.trim();
        
        // Check whitelist patterns
        let selector_lower = selector_trimmed.to_lowercase();
        for pattern in &self.whitelist_patterns {
            if selector_lower.contains(pattern) {
                return true;
            }
        }

        // Keep pseudo-elements and pseudo-classes always
        if selector_lower.contains("::") || selector_lower.contains(":hover") || 
           selector_lower.contains(":focus") || selector_lower.contains(":active") ||
           selector_lower.contains(":before") || selector_lower.contains(":after") ||
           selector_lower.contains(":nth") || selector_lower.contains(":first") ||
           selector_lower.contains(":last") || selector_lower.contains(":not") {
            return true;
        }

        // Keep @keyframes, @font-face, @media
        if selector_lower.starts_with('@') {
            return true;
        }

        // Parse the selector into parts (.class, #id, tagname)
        // For complex selectors like ".parent .child", check if ANY part is used
        let parts = self.parse_selector_parts(selector_trimmed);
        
        for part in parts {
            if self.used_selectors.contains(&part) {
                return true;
            }
        }

        // If selector starts with element name, check if that element exists
        let first_char = selector_trimmed.chars().next().unwrap_or(' ');
        if first_char.is_alphabetic() {
            // This is an element selector like "body", "div", etc.
            let tag = selector_trimmed.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
            if self.used_selectors.contains(&tag.to_lowercase()) {
                return true;
            }
        }

        false
    }

    /// Parse a CSS selector into its component parts
    fn parse_selector_parts(&self, selector: &str) -> Vec<String> {
        let mut parts = Vec::new();
        
        // Split by combinators and whitespace
        let tokens: Vec<&str> = selector.split(|c: char| {
            c.is_whitespace() || c == '>' || c == '+' || c == '~'
        }).collect();

        for token in tokens {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            // Extract classes (.class)
            for class_match in token.split('.').skip(1) {
                let class_name = class_match.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                    .next()
                    .unwrap_or("");
                if !class_name.is_empty() {
                    parts.push(format!(".{}", class_name));
                }
            }

            // Extract IDs (#id)
            for id_match in token.split('#').skip(1) {
                let id_name = id_match.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                    .next()
                    .unwrap_or("");
                if !id_name.is_empty() {
                    parts.push(format!("#{}", id_name));
                }
            }

            // Extract element name (first part before . or #)
            let element = token.split(|c| c == '.' || c == '#' || c == '[' || c == ':')
                .next()
                .unwrap_or("");
            if !element.is_empty() && element.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                parts.push(element.to_lowercase());
            }
        }

        parts
    }

    /// Remove unused CSS rules - aggressive tree-shaking
    pub fn remove_unused_css(&self, css: &str) -> Result<String, String> {
        // Parse CSS into rules using a simple regex-based approach
        // This handles: .class { }, #id { }, tag { }, .class .child { }
        let mut result = String::with_capacity(css.len());
        let mut remaining = css;
        
        // Track how many bytes we remove
        let original_len = css.len();
        let mut removed_rules = 0;
        let mut kept_rules = 0;

        while !remaining.is_empty() {
            // Look for start of a rule (selector {) or at-rule (@)
            if let Some(selector_end) = remaining.find('{') {
                let selector = remaining[..selector_end].trim();
                
                // Handle at-rules (@media, @keyframes, @font-face)
                if selector.starts_with('@') {
                    // Find the matching closing brace (handle nested braces)
                    if let Some(rule_content) = self.extract_at_rule(remaining) {
                        result.push_str(&rule_content);
                        remaining = &remaining[rule_content.len()..];
                        kept_rules += 1;
                        continue;
                    }
                }
                
                // Find the closing brace for this rule
                let rule_start = selector_end;
                if let Some(rule_end) = remaining[rule_start..].find('}') {
                    let full_rule = &remaining[..rule_start + rule_end + 1];
                    
                    // Check if selector is used
                    if self.is_selector_used(selector) {
                        // Keep the rule, but minify it
                        result.push_str(selector.split_whitespace().collect::<Vec<_>>().join(" ").as_str());
                        result.push('{');
                        let body = &remaining[selector_end + 1..rule_start + rule_end];
                        result.push_str(self.minify_rule_body(body).as_str());
                        result.push('}');
                        kept_rules += 1;
                    } else {
                        // Skip this rule - it's unused
                        removed_rules += 1;
                    }
                    
                    remaining = &remaining[full_rule.len()..];
                } else {
                    // Malformed CSS, keep remaining as-is
                    result.push_str(remaining);
                    break;
                }
            } else {
                // No more rules, append remaining content
                result.push_str(remaining.trim());
                break;
            }
        }

        tracing::debug!(
            "CSS tree-shake: {} rules removed, {} kept, {}% reduction",
            removed_rules,
            kept_rules,
            if original_len > result.len() {
                (original_len - result.len()) * 100 / original_len
            } else {
                0
            }
        );

        Ok(result)
    }

    /// Extract at-rule including nested braces
    fn extract_at_rule(&self, css: &str) -> Option<String> {
        let mut brace_count = 0;
        let mut in_rule = false;
        let mut end_pos = 0;

        for (i, c) in css.chars().enumerate() {
            match c {
                '{' => {
                    brace_count += 1;
                    in_rule = true;
                }
                '}' => {
                    brace_count -= 1;
                    if in_rule && brace_count == 0 {
                        end_pos = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        if end_pos > 0 {
            Some(css[..end_pos].to_string())
        } else {
            None
        }
    }

    /// Minify a CSS rule body (remove extra whitespace)
    fn minify_rule_body(&self, body: &str) -> String {
        body.split(';')
            .map(|prop| prop.trim())
            .filter(|prop| !prop.is_empty())
            .collect::<Vec<_>>()
            .join(";")
            + ";"
    }

    /// Extract critical (above-the-fold) CSS
    /// For MVP: Extract CSS for elements visible in first viewport
    pub fn extract_critical_css(&self, css: &str, html: &str) -> Result<CssResult, String> {
        let original_size = css.len();
        
        // Parse and minify the CSS
        let opts = ParserOptions::default();
        let printer_opts = PrinterOptions {
            minify: true,
            ..Default::default()
        };

        let stylesheet = StyleSheet::parse(css, opts)
            .map_err(|e| format!("CSS parse error: {:?}", e))?;

        let minified = stylesheet.to_css(printer_opts)
            .map_err(|e| format!("CSS print error: {:?}", e))?;

        let optimized_size = minified.code.len();
        let unused_removed = if original_size > 0 {
            ((original_size - optimized_size) as f64 / original_size as f64) * 100.0
        } else {
            0.0
        };

        // For MVP: All CSS is considered "critical" 
        // Full implementation would analyze viewport and fold position
        Ok(CssResult {
            critical_css: minified.code.clone(),
            deferred_css: String::new(),
            original_size,
            optimized_size,
            unused_removed_percent: (unused_removed * 10.0).round() / 10.0,
        })
    }
}

/// Minify CSS using lightningcss
pub fn minify_css(css: &str) -> Result<String, String> {
    let opts = ParserOptions::default();
    let printer_opts = PrinterOptions {
        minify: true,
        ..Default::default()
    };

    let stylesheet = StyleSheet::parse(css, opts)
        .map_err(|e| format!("CSS parse error: {:?}", e))?;

    let result = stylesheet.to_css(printer_opts)
        .map_err(|e| format!("CSS print error: {:?}", e))?;

    Ok(result.code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minify_css() {
        let css = r#"
            .test {
                color: #ffffff;
                margin: 0px;
            }
        "#;
        
        let result = minify_css(css).unwrap();
        assert!(result.len() < css.len());
        assert!(result.contains(".test"));
    }

    #[test]
    fn test_extract_selectors() {
        let html = r#"<div class="hero main" id="content"><p class="text">Hello</p></div>"#;
        let mut optimizer = CssOptimizer::new();
        optimizer.extract_used_selectors(html);
        
        assert!(optimizer.used_selectors.contains(".hero"));
        assert!(optimizer.used_selectors.contains(".main"));
        assert!(optimizer.used_selectors.contains("#content"));
        assert!(optimizer.used_selectors.contains(".text"));
    }
}
