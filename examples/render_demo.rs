/// Demo: Fetch real websites → Boa JS eval → Blitz render → PNG
/// Bevisar end-to-end att JS-modifierad DOM renderas korrekt av Blitz
use std::fs;
use std::process::Command;

fn fetch_html(url: &str) -> String {
    let output = Command::new("curl")
        .args(["-sL", "--max-time", "10", "-A", "Mozilla/5.0", url])
        .output()
        .expect("curl måste finnas");
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn main() {
    let out_dir = "/tmp/aether_demo";
    fs::create_dir_all(out_dir).unwrap();

    // ─── Test 1: Ren Blitz-rendering av example.com ─────────────────
    println!("\n=== Test 1: example.com — ren Blitz-rendering ===");
    let html = fetch_html("https://example.com");
    println!("Hämtade {} tecken HTML", html.len());

    match aether_agent::render_html_to_png(&html, "https://example.com", 1280, 800, true) {
        Ok(png) => {
            let path = format!("{}/01_example_com.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
        Err(e) => println!("✗ Render-fel: {}", e),
    }

    // ─── Test 2: example.com + JS som modifierar DOM ────────────────
    println!("\n=== Test 2: example.com + JS-modifikation ===");
    let js = r#"
        document.querySelector("h1").textContent = "MODIFIED BY BOA JS ENGINE";
        var p = document.querySelector("p");
        if (p) { p.textContent = "This content was dynamically changed by the Boa JavaScript sandbox."; }
    "#;
    let result_json = aether_agent::render_with_js(&html, js, "https://example.com", 1280, 800);
    let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    println!(
        "Mutationer: {}, JS-tid: {}µs, Total: {}ms",
        result["mutation_count"], result["eval_time_us"], result["total_ms"]
    );
    if let Some(err) = result["js_error"].as_str() {
        println!("JS-fel: {}", err);
    }
    if let Some(b64) = result["png_base64"].as_str() {
        if !b64.is_empty() {
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            let path = format!("{}/02_example_com_js_modified.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
    }

    // ─── Test 3: Hacker News — ren rendering ────────────────────────
    println!("\n=== Test 3: Hacker News — ren Blitz-rendering ===");
    let hn_html = fetch_html("https://news.ycombinator.com");
    println!("Hämtade {} tecken HTML", hn_html.len());

    match aether_agent::render_html_to_png(&hn_html, "https://news.ycombinator.com", 1280, 900, true) {
        Ok(png) => {
            let path = format!("{}/03_hackernews.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
        Err(e) => println!("✗ Render-fel: {}", e),
    }

    // ─── Test 4: Hacker News + JS som highlightar första posten ─────
    println!("\n=== Test 4: Hacker News + JS-highlight ===");
    let hn_js = r#"
        var title = document.querySelector(".titleline");
        if (title) {
            title.setAttribute("style", "background: yellow; padding: 10px; font-size: 24px;");
        }
    "#;
    let hn_result_json = aether_agent::render_with_js(&hn_html, hn_js, "https://news.ycombinator.com", 1280, 900);
    let hn_result: serde_json::Value = serde_json::from_str(&hn_result_json).unwrap();
    println!(
        "Mutationer: {}, JS-tid: {}µs, Total: {}ms",
        hn_result["mutation_count"], hn_result["eval_time_us"], hn_result["total_ms"]
    );
    if let Some(b64) = hn_result["png_base64"].as_str() {
        if !b64.is_empty() {
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            let path = format!("{}/04_hackernews_js_highlight.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
    }

    // ─── Test 5: Books.toscrape.com — riktig e-commerce ─────────────
    println!("\n=== Test 5: Books to Scrape — Blitz-rendering ===");
    let books_html = fetch_html("https://books.toscrape.com");
    println!("Hämtade {} tecken HTML", books_html.len());

    match aether_agent::render_html_to_png(&books_html, "https://books.toscrape.com", 1280, 900, true) {
        Ok(png) => {
            let path = format!("{}/05_books_toscrape.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
        Err(e) => println!("✗ Render-fel: {}", e),
    }

    // ─── Test 6: Books.toscrape + JS prisjustering ──────────────────
    println!("\n=== Test 6: Books to Scrape + JS-prisändring ===");
    let books_js = r#"
        var prices = document.querySelectorAll(".price_color");
        for (var i = 0; i < prices.length; i++) {
            prices[i].textContent = "£0.00 SALE!";
            prices[i].setAttribute("style", "color: red; font-weight: bold; font-size: 18px;");
        }
        var h1 = document.querySelector("h1");
        if (h1) { h1.textContent = "BOA JS ENGINE — ALL BOOKS ON SALE!"; }
    "#;
    let books_result_json = aether_agent::render_with_js(&books_html, books_js, "https://books.toscrape.com", 1280, 900);
    let books_result: serde_json::Value = serde_json::from_str(&books_result_json).unwrap();
    println!(
        "Mutationer: {}, JS-tid: {}µs, Total: {}ms, Modifierad HTML: {} tecken",
        books_result["mutation_count"],
        books_result["eval_time_us"],
        books_result["total_ms"],
        books_result["modified_html_length"]
    );
    if let Some(b64) = books_result["png_base64"].as_str() {
        if !b64.is_empty() {
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            let path = format!("{}/06_books_toscrape_js_sale.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
    }

    // ─── Test 7: Bygg en hel sida med bara JS ──────────────────────
    println!("\n=== Test 7: Sida byggd helt med JS (SPA-liknande) ===");
    let empty_html = "<html><head><style>body{font-family:sans-serif;margin:40px}h1{color:#2563eb}.card{border:1px solid #e5e7eb;border-radius:8px;padding:16px;margin:8px 0;background:#f9fafb}.price{color:#059669;font-size:20px;font-weight:bold}.btn{background:#2563eb;color:white;padding:8px 16px;border:none;border-radius:4px;cursor:pointer}</style></head><body><div id=\"app\"></div></body></html>";
    let spa_js = r#"
        var app = document.getElementById("app");

        var h1 = document.createElement("h1");
        h1.textContent = "AetherAgent Shop";
        app.appendChild(h1);

        var subtitle = document.createElement("p");
        subtitle.textContent = "Rendered entirely by Boa JS + Blitz — no server-side HTML!";
        app.appendChild(subtitle);

        var products = [
            { name: "Quantum Widget", price: "$49.99", desc: "Next-gen quantum computing widget" },
            { name: "AI Accelerator", price: "$199.99", desc: "Neural network training chip" },
            { name: "Rust Compiler Pro", price: "$79.99", desc: "Blazingly fast compilation" }
        ];

        for (var i = 0; i < products.length; i++) {
            var card = document.createElement("div");
            card.setAttribute("class", "card");

            var name = document.createElement("h2");
            name.textContent = products[i].name;
            card.appendChild(name);

            var desc = document.createElement("p");
            desc.textContent = products[i].desc;
            card.appendChild(desc);

            var price = document.createElement("span");
            price.setAttribute("class", "price");
            price.textContent = products[i].price;
            card.appendChild(price);

            var btn = document.createElement("button");
            btn.setAttribute("class", "btn");
            btn.textContent = "Add to Cart";
            card.appendChild(btn);

            app.appendChild(card);
        }
    "#;
    let spa_result_json = aether_agent::render_with_js(empty_html, spa_js, "https://shop.example.com", 1280, 900);
    let spa_result: serde_json::Value = serde_json::from_str(&spa_result_json).unwrap();
    println!(
        "Mutationer: {}, JS-tid: {}µs, Total: {}ms, Modifierad HTML: {} tecken",
        spa_result["mutation_count"],
        spa_result["eval_time_us"],
        spa_result["total_ms"],
        spa_result["modified_html_length"]
    );
    if let Some(err) = spa_result["js_error"].as_str() {
        println!("JS-fel: {}", err);
    }
    if let Some(b64) = spa_result["png_base64"].as_str() {
        if !b64.is_empty() {
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            let path = format!("{}/07_spa_built_by_js.png", out_dir);
            fs::write(&path, &png).unwrap();
            println!("✓ Sparade {} ({} bytes)", path, png.len());
        }
    }

    println!("\n=== KLART ===");
    println!("Alla screenshots sparade i {}/", out_dir);
    println!("Visa med: ls -la {}/", out_dir);
}
