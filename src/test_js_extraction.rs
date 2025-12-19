use scraper::{Html, Selector};

fn extract_js_sources(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("script[src]").unwrap();

    document
        .select(&selector)
        .filter_map(|element| element.value().attr("src"))
        .filter(|src| !src.starts_with("data:") && !src.is_empty())
        .map(|src| src.to_string())
        .collect()
}

fn find_script_tag_start(html: &str, url: &str) -> Option<usize> {
    let lower_html = html.to_lowercase();
    let lower_url = url.to_lowercase();
    
    // Look for src="url" or src='url'
    for pattern in [format!("src=\"{}\"", lower_url), format!("src='{}'", lower_url)] {
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

fn main() {
    let html = r#"<script defer type="text/javascript" src="https://pillarshoteldv.wpenginepowered.com/wp-includes/js/jquery/jquery.min.js?ver=3.7.1" id="jquery-core-js"></script>"#;
    let url = "https://pillarshoteldv.wpenginepowered.com/wp-includes/js/jquery/jquery.min.js?ver=3.7.1";
    
    println!("Testing extraction...");
    let sources = extract_js_sources(html);
    println!("Found {} sources", sources.len());
    for src in &sources {
        println!(" - {}", src);
    }
    
    println!("\nTesting find position...");
    match find_script_tag_start(html, url) {
        Some(pos) => println!("Found at position: {}", pos),
        None => println!("NOT FOUND! Pattern matching failed."),
    }
}
