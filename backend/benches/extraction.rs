use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use essence::format::metadata::extract_metadata;
use scraper::Html;
use std::fs;
use std::path::PathBuf;

/// Load HTML fixture for benchmarking
fn load_fixture(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    fs::read_to_string(&path).unwrap_or_else(|_| {
        // Fallback to synthetic HTML
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
    <meta name="description" content="Test description">
    <meta property="og:title" content="OG Title">
    <meta property="og:description" content="OG Description">
    <meta property="og:image" content="https://example.com/image.jpg">
</head>
<body>
    <article>
        <h1>Main Content</h1>
        <p>Article paragraph 1</p>
        <p>Article paragraph 2</p>
    </article>
    <aside>Sidebar content</aside>
    <footer>Footer content</footer>
</body>
</html>"#
            .to_string()
    })
}

fn bench_metadata_extraction_simple(c: &mut Criterion) {
    let html = r#"
        <html>
        <head>
            <title>Simple Page</title>
            <meta name="description" content="A simple test page">
        </head>
        <body><p>Content</p></body>
        </html>
    "#;
    let document = Html::parse_document(html);

    c.bench_function("metadata_simple", |b| {
        b.iter(|| extract_metadata(black_box(&document), "https://example.com"))
    });
}

fn bench_metadata_extraction_rich(c: &mut Criterion) {
    let html = load_fixture("article.html");
    let document = Html::parse_document(&html);

    c.bench_function("metadata_rich", |b| {
        b.iter(|| extract_metadata(black_box(&document), "https://example.com/article"))
    });
}

fn bench_metadata_extraction_with_og(c: &mut Criterion) {
    let html = r#"
        <html>
        <head>
            <title>OG Test Page</title>
            <meta name="description" content="Description">
            <meta property="og:title" content="OG Title">
            <meta property="og:description" content="OG Description">
            <meta property="og:image" content="https://example.com/og.jpg">
            <meta property="og:url" content="https://example.com">
            <meta property="og:type" content="article">
            <meta property="article:published_time" content="2025-01-01">
            <meta property="article:author" content="Test Author">
        </head>
        <body><p>Content</p></body>
        </html>
    "#;
    let document = Html::parse_document(html);

    c.bench_function("metadata_with_og", |b| {
        b.iter(|| extract_metadata(black_box(&document), "https://example.com"))
    });
}

fn bench_link_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("link_extraction");

    for link_count in [10, 50, 100, 500].iter() {
        let mut html = String::from("<html><body>");
        for i in 0..*link_count {
            html.push_str(&format!(
                r#"<a href="https://example.com/page-{}">Link {}</a>"#,
                i, i
            ));
        }
        html.push_str("</body></html>");

        let document = Html::parse_document(&html);
        group.bench_with_input(
            BenchmarkId::from_parameter(link_count),
            link_count,
            |b, _| {
                b.iter(|| {
                    // Extract links using scraper
                    let selector = scraper::Selector::parse("a[href]").unwrap();
                    let _links: Vec<_> = black_box(&document)
                        .select(&selector)
                        .filter_map(|el| el.value().attr("href"))
                        .collect();
                })
            },
        );
    }

    group.finish();
}

fn bench_image_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("image_extraction");

    for image_count in [10, 50, 100, 500].iter() {
        let mut html = String::from("<html><body>");
        for i in 0..*image_count {
            html.push_str(&format!(
                r#"<img src="https://example.com/image-{}.jpg" alt="Image {}">"#,
                i, i
            ));
        }
        html.push_str("</body></html>");

        let document = Html::parse_document(&html);
        group.bench_with_input(
            BenchmarkId::from_parameter(image_count),
            image_count,
            |b, _| {
                b.iter(|| {
                    // Extract images using scraper
                    let selector = scraper::Selector::parse("img[src]").unwrap();
                    let _images: Vec<_> = black_box(&document)
                        .select(&selector)
                        .filter_map(|el| el.value().attr("src"))
                        .collect();
                })
            },
        );
    }

    group.finish();
}

fn bench_main_content_extraction(c: &mut Criterion) {
    let html = r#"
        <html>
        <body>
            <nav>Navigation menu</nav>
            <header>Site header</header>
            <main>
                <article>
                    <h1>Main Article</h1>
                    <p>Important content paragraph 1</p>
                    <p>Important content paragraph 2</p>
                    <p>Important content paragraph 3</p>
                </article>
            </main>
            <aside>Sidebar advertisements</aside>
            <footer>Footer content</footer>
        </body>
        </html>
    "#;
    let document = Html::parse_document(html);

    c.bench_function("main_content_extraction", |b| {
        b.iter(|| {
            // Extract main content using selectors
            let main_selector = scraper::Selector::parse("main, article, [role=main]").unwrap();
            let _content: Vec<_> = black_box(&document)
                .select(&main_selector)
                .map(|el| el.text().collect::<Vec<_>>().join(" "))
                .collect();
        })
    });
}

criterion_group!(
    benches,
    bench_metadata_extraction_simple,
    bench_metadata_extraction_rich,
    bench_metadata_extraction_with_og,
    bench_link_extraction,
    bench_image_extraction,
    bench_main_content_extraction
);

criterion_main!(benches);
