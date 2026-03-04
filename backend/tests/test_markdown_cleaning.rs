use essence::format::markdown::html_to_markdown;

#[test]
fn test_token_reduction_demonstration() {
    // Sample HTML with images and various tags
    let html = r#"
        <!DOCTYPE html>
        <html>
        <body>
            <div class="header">
                <img src="https://github.com/logo.png" alt="GitHub Logo">
                <h1>Documentation</h1>
            </div>
            <main>
                <p>Content with <span>formatting</span>.</p>
                <img src="diagram.png" alt="Diagram" width="800">
                <ul>
                    <li><img src="check.png" alt="Check"> Item 1</li>
                    <li><img src="check.png" alt="Check"> Item 2</li>
                </ul>
            </main>
        </body>
        </html>
    "#;

    // Convert to markdown
    let result = html_to_markdown(html, "https://example.com", false);
    assert!(result.is_ok());

    let markdown = result.unwrap();

    println!("\n=== CLEANED MARKDOWN OUTPUT ===");
    println!("{}", markdown);
    println!("\n=== STATS ===");
    println!("Character count: {}", markdown.len());

    // Verify image conversion
    assert!(
        markdown.contains("![GitHub Logo](https://github.com/logo.png)")
            || markdown.contains("![](https://github.com/logo.png)"),
        "Should contain markdown image format for GitHub logo"
    );

    assert!(
        markdown.contains("![Diagram](diagram.png)") || markdown.contains("![](diagram.png)"),
        "Should contain markdown image format for diagram"
    );

    // Verify NO HTML tags remain
    assert!(!markdown.contains("<img"), "Should not contain <img tags");
    assert!(!markdown.contains("<div"), "Should not contain <div tags");
    assert!(!markdown.contains("<span"), "Should not contain <span tags");
    assert!(
        !markdown.contains("src="),
        "Should not contain HTML attributes"
    );
    assert!(
        !markdown.contains("width="),
        "Should not contain HTML attributes"
    );

    // Verify content is preserved
    assert!(
        markdown.contains("Documentation"),
        "Should preserve heading text"
    );
    assert!(markdown.contains("Item 1"), "Should preserve list items");
    assert!(
        markdown.contains("formatting"),
        "Should preserve inline text"
    );

    println!("\n=== IMAGE CONVERSION CHECK ===");
    println!("Markdown images found: {}", markdown.matches("![").count());
    println!("HTML img tags found: {}", markdown.matches("<img").count());
}

#[test]
fn test_token_count_comparison() {
    // Compare token efficiency: HTML vs Markdown
    let html_img = r#"<img src="https://example.com/very-long-path/image.png" alt="Description" width="300" height="200" class="image-class">"#;

    // Using our cleaning function
    let result = html_to_markdown(html_img, "https://example.com", false);
    assert!(result.is_ok());
    let cleaned = result.unwrap();

    println!("\n=== TOKEN EFFICIENCY TEST ===");
    println!("HTML version length: {} chars", html_img.len());
    println!("HTML: {}", html_img);
    println!("\nMarkdown version length: {} chars", cleaned.len());
    println!("Markdown: {}", cleaned);

    // Markdown should be more compact (no width/height/class attributes)
    // and more LLM-friendly
    assert!(cleaned.contains("!["), "Should be markdown image format");
    assert!(
        !cleaned.contains("width"),
        "Should not have HTML attributes"
    );
    assert!(
        !cleaned.contains("class"),
        "Should not have HTML attributes"
    );

    // Calculate rough savings
    let savings = html_img.len() as i32 - cleaned.len() as i32;
    println!(
        "\nCharacter savings: {} ({:.1}% reduction)",
        savings,
        (savings as f32 / html_img.len() as f32) * 100.0
    );
}

#[test]
fn test_github_readme_style_page() {
    // Simulate a GitHub README with badges and images
    let html = r#"
        <!DOCTYPE html>
        <html>
        <body>
            <div class="markdown-body">
                <h1>Project Name</h1>
                
                <p>
                    <img src="https://img.shields.io/badge/version-1.0.0-blue" alt="Version Badge">
                    <img src="https://img.shields.io/badge/license-MIT-green" alt="License Badge">
                    <img src="https://img.shields.io/badge/build-passing-success" alt="Build Badge">
                </p>
                
                <h2>Features</h2>
                <ul>
                    <li><img src="checkmark.svg" alt="check" width="16" height="16"> Fast and efficient</li>
                    <li><img src="checkmark.svg" alt="check" width="16" height="16"> Easy to use</li>
                    <li><img src="checkmark.svg" alt="check" width="16" height="16"> Well documented</li>
                </ul>
                
                <h2>Architecture</h2>
                <p>Here's how it works:</p>
                <img src="https://raw.githubusercontent.com/user/repo/main/docs/architecture-diagram.png" 
                     alt="Architecture Diagram" 
                     width="800" 
                     height="600"
                     style="max-width: 100%;">
                
                <h2>Screenshots</h2>
                <div class="screenshots">
                    <img src="screenshot1.png" alt="Dashboard View" width="400" height="300" class="screenshot">
                    <img src="screenshot2.png" alt="Settings Panel" width="400" height="300" class="screenshot">
                    <img src="screenshot3.png" alt="Report View" width="400" height="300" class="screenshot">
                </div>
                
                <footer>
                    <p>Made with ❤️ by the team</p>
                </footer>
            </div>
        </body>
        </html>
    "#;

    let result = html_to_markdown(html, "https://github.com", false);
    assert!(result.is_ok());

    let markdown = result.unwrap();

    println!("\n=== GITHUB README STYLE TEST ===");
    println!("{}", markdown);
    println!("\n=== COMPREHENSIVE STATS ===");
    println!("HTML length: {} chars", html.len());
    println!("Markdown length: {} chars", markdown.len());
    println!(
        "Reduction: {} chars ({:.1}%)",
        html.len() - markdown.len(),
        ((html.len() - markdown.len()) as f32 / html.len() as f32) * 100.0
    );

    // Count elements
    let md_images = markdown.matches("![").count();
    let html_tags = html.matches("<img").count();

    println!("\n=== ELEMENT COUNTS ===");
    println!("HTML img tags: {}", html_tags);
    println!("Markdown images: {}", md_images);
    println!("Images converted: {}/{}", md_images, html_tags);

    // Verify all images converted
    assert_eq!(md_images, html_tags, "All images should be converted");
    assert!(!markdown.contains("<img"), "No HTML img tags should remain");
    assert!(!markdown.contains("<div"), "No div tags should remain");
    assert!(
        !markdown.contains("width="),
        "No width attributes should remain"
    );
    assert!(
        !markdown.contains("height="),
        "No height attributes should remain"
    );
    assert!(
        !markdown.contains("class="),
        "No class attributes should remain"
    );
    assert!(
        !markdown.contains("style="),
        "No style attributes should remain"
    );

    // Verify content preserved
    assert!(markdown.contains("Project Name"));
    assert!(markdown.contains("Features"));
    assert!(markdown.contains("Fast and efficient"));
    assert!(markdown.contains("Architecture"));
}

#[test]
fn test_documentation_page_with_inline_images() {
    // Test inline images mixed with text
    let html = r#"
        <div class="docs">
            <p>
                Click the <img src="settings-icon.png" alt="settings icon" width="20" height="20"> settings icon 
                to open preferences. Then select <img src="save-icon.png" alt="save" width="20"> to save your changes.
            </p>
            <p>
                The <img src="warning.png" alt="warning"> warning indicator appears when there's an issue.
            </p>
        </div>
    "#;

    let result = html_to_markdown(html, "https://docs.example.com", false);
    assert!(result.is_ok());

    let markdown = result.unwrap();

    println!("\n=== INLINE IMAGES TEST ===");
    println!("{}", markdown);

    // Verify inline images are converted
    assert!(markdown.contains("![settings icon](settings-icon.png)"));
    assert!(markdown.contains("![save](save-icon.png)"));
    assert!(markdown.contains("![warning](warning.png)"));

    // Verify no HTML remains
    assert!(!markdown.contains("<img"));
    assert!(!markdown.contains("<div"));
    assert!(!markdown.contains("<p>"));

    // Text should be preserved
    assert!(markdown.contains("Click the"));
    assert!(markdown.contains("to open preferences"));
    assert!(markdown.contains("save your changes"));
}
