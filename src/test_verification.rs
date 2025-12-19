#[cfg(test)]
mod tests {
    use crate::optimizer;
    use crate::handlers::OptimizeOptions;

    #[test]
    fn test_html_optimization() {
        let html_input = r#"
<!DOCTYPE html><html><head>
    <!-- remove me -->
    <style> 
        .foo { color: red; } 
    </style>
    <script src=https://example.com/script.js></script>
</head>
<body>
    <div class="foo"></div>
    <h1>  Test   Spaces  </h1>
    <script>
        var x = 1;
        // comment
        console.log(x);
    </script>
</body>
</html>"#;

        let options = OptimizeOptions {
            minify_html: true,
            minify_css: true,
            minify_js: true,
            remove_unused_css: false,
            convert_webp: false,
            resize_images: false,
            defer_js: false,
            lazy_images: false,
            optimize_resources: false,
        };

        let result = optimizer::optimize_html(html_input, "http://localhost", &options).expect("Optimization failed");
        let optimized = result.html;

        println!("Original: {}", html_input);
        println!("Optimized: {}", optimized);

        // Checks
        assert!(!optimized.contains("remove me"), "Comments should be removed");
        assert!(!optimized.contains("  Test   Spaces  "), "Whitespace should be collapsed");
        // Check CSS (minify-html minifies inline CSS)
        assert!(optimized.contains(".foo{color:red}"), "CSS should be minified");
        
        // Inline JS minification check
        // verify it doesn't contain "// comment"
        assert!(!optimized.contains("// comment"), "JS comments should be removed");
        
        // Unquoted attribute check.
        // It should match either unquoted or quoted.
        // minify-html usually keeps unquoted if safe.
        // src=https://example.com/script.js is safe.
        assert!(optimized.contains("src=https://example.com/script.js") || optimized.contains("src=\"https://example.com/script.js\""), "Script src should be present");
    }
}
