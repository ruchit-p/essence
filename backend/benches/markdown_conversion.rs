use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use essence::format::markdown::convert_to_markdown;
use std::fs;
use std::path::PathBuf;

/// Load HTML fixture for benchmarking
fn load_fixture(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(name);
    fs::read_to_string(&path).unwrap_or_else(|_| {
        // Fallback to simple HTML if fixture doesn't exist
        format!(
            r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
    <h1>Test Heading</h1>
    <p>This is a test paragraph with <strong>bold</strong> and <em>italic</em> text.</p>
    <ul>
        <li>Item 1</li>
        <li>Item 2</li>
        <li>Item 3</li>
    </ul>
    <pre><code>fn main() {{ println!("Hello, world!"); }}</code></pre>
</body>
</html>"#
        )
    })
}

/// Generate synthetic HTML with varying complexity
fn generate_html(paragraphs: usize, lists: usize, code_blocks: usize) -> String {
    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head><title>Benchmark Test Page</title></head>
<body>
    <h1>Main Heading</h1>
"#,
    );

    for i in 0..paragraphs {
        html.push_str(&format!(
            "<p>This is paragraph {} with some <strong>bold</strong> and <em>italic</em> text. It contains multiple sentences to simulate real content.</p>\n",
            i
        ));
    }

    for i in 0..lists {
        html.push_str(&format!("<h2>List {}</h2>\n<ul>\n", i));
        for j in 0..5 {
            html.push_str(&format!("<li>List {} item {}</li>\n", i, j));
        }
        html.push_str("</ul>\n");
    }

    for i in 0..code_blocks {
        html.push_str(&format!(
            "<pre><code>fn example_{}() {{\n    println!(\"Code block {}\");\n}}</code></pre>\n",
            i, i
        ));
    }

    html.push_str("</body>\n</html>");
    html
}

fn bench_markdown_conversion_simple(c: &mut Criterion) {
    let html = r#"<h1>Simple Page</h1><p>This is a simple page.</p>"#;

    c.bench_function("markdown_simple", |b| {
        b.iter(|| convert_to_markdown(black_box(html)))
    });
}

fn bench_markdown_conversion_complex(c: &mut Criterion) {
    let html = load_fixture("article.html");

    c.bench_function("markdown_complex", |b| {
        b.iter(|| convert_to_markdown(black_box(&html)))
    });
}

fn bench_markdown_by_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_by_size");

    for size in [10, 50, 100, 500].iter() {
        let html = generate_html(*size, *size / 5, *size / 10);
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| convert_to_markdown(black_box(&html)))
        });
    }

    group.finish();
}

fn bench_markdown_with_lists(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_with_lists");

    for list_count in [5, 10, 20, 50].iter() {
        let html = generate_html(10, *list_count, 2);
        group.bench_with_input(BenchmarkId::from_parameter(list_count), list_count, |b, _| {
            b.iter(|| convert_to_markdown(black_box(&html)))
        });
    }

    group.finish();
}

fn bench_markdown_with_code(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_with_code");

    for code_count in [5, 10, 20, 50].iter() {
        let html = generate_html(10, 2, *code_count);
        group.bench_with_input(BenchmarkId::from_parameter(code_count), code_count, |b, _| {
            b.iter(|| convert_to_markdown(black_box(&html)))
        });
    }

    group.finish();
}

fn bench_markdown_with_tables(c: &mut Criterion) {
    let html = r#"
        <table>
            <thead>
                <tr><th>Column 1</th><th>Column 2</th><th>Column 3</th></tr>
            </thead>
            <tbody>
                <tr><td>Row 1 Col 1</td><td>Row 1 Col 2</td><td>Row 1 Col 3</td></tr>
                <tr><td>Row 2 Col 1</td><td>Row 2 Col 2</td><td>Row 2 Col 3</td></tr>
                <tr><td>Row 3 Col 1</td><td>Row 3 Col 2</td><td>Row 3 Col 3</td></tr>
            </tbody>
        </table>
    "#;

    c.bench_function("markdown_tables", |b| {
        b.iter(|| convert_to_markdown(black_box(html)))
    });
}

criterion_group!(
    benches,
    bench_markdown_conversion_simple,
    bench_markdown_conversion_complex,
    bench_markdown_by_size,
    bench_markdown_with_lists,
    bench_markdown_with_code,
    bench_markdown_with_tables
);

criterion_main!(benches);
