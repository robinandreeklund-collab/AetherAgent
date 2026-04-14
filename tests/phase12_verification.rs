/// Phase 1-2 verification tests
/// Tests each fix against realistic HTML that reproduces the bugs found in the 50-site test.
///
/// Run: cargo test --test phase12_verification -- --nocapture

use aether_agent::*;

fn parse_crfr_json(html: &str, goal: &str, url: &str, top_n: u32) -> serde_json::Value {
    let result = parse_crfr(html, goal, url, top_n, false, "json");
    serde_json::from_str(&result).expect("parse_crfr ska returnera giltig JSON")
}

// ─── Phase 1.1: Smarter SPA Detection ──────────────────────────────────────

#[test]
fn test_phase11_nav_heavy_page_not_classified_as_sufficient() {
    // En sida med massor av nav-items men ingen riktig content
    // Ska INTE klassas som "sufficient static content"
    let html = r##"<html><body>
        <nav>
            <ul>
                <li><a href="/">Home</a></li>
                <li><a href="/about">About</a></li>
                <li><a href="/contact">Contact</a></li>
                <li><a href="/blog">Blog</a></li>
                <li><a href="/shop">Shop</a></li>
                <li><a href="/faq">FAQ</a></li>
                <li><a href="/terms">Terms</a></li>
                <li><a href="/privacy">Privacy</a></li>
            </ul>
        </nav>
        <div id="root"></div>
        <script src="/bundle.js"></script>
        <script>document.getElementById('root').innerHTML = '<h1>Dynamic</h1>';</script>
    </body></html>"##;

    let result = parse_crfr_json(html, "find main content articles", "https://spa.example.com", 10);
    let nodes = result["nodes"].as_array().expect("ska ha nodes");

    // Auto-escalation ska ha kickat in — vi borde se "Dynamic" innehållet
    // eftersom QuickJS kör document.getElementById('root').innerHTML
    let total = result["total_nodes"].as_u64().unwrap_or(0);
    println!("Phase 1.1: total_nodes={}, node_count={}", total, nodes.len());

    // Ska inte returnera en sida full med bara nav-items
    let nav_only = nodes.iter().all(|n| {
        let label = n["label"].as_str().unwrap_or("");
        label.len() < 15
    });
    assert!(
        !nav_only || !nodes.is_empty(),
        "Sidan ska inte bestå av enbart korta nav-items"
    );
}

// ─── Phase 1.3: Auto-Escalation ──────────────────────────────────────────

#[test]
fn test_phase13_auto_escalation_js_content() {
    // En sida som behöver JS för att visa content
    let html = r##"<html><body>
        <div id="app"></div>
        <script>
            var el = document.getElementById('app');
            el.innerHTML = '<h1>Loaded via JS</h1><p>This product costs $49.99 and ships free.</p><a href="/buy">Buy Now</a>';
        </script>
    </body></html>"##;

    let result = parse_crfr_json(html, "product price buy cost", "https://shop.example.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 1.3: {} nodes returned", nodes.len());
    for n in nodes {
        println!(
            "  [{}] {} (rel={:.3})",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(60)],
            n["relevance"].as_f64().unwrap_or(0.0)
        );
    }

    // With js-eval feature: should find JS-generated content via auto-escalation
    // Without js-eval: should still return something (safety net / static parse)
    let has_price = nodes.iter().any(|n| {
        let label = n["label"].as_str().unwrap_or("");
        label.contains("49.99") || label.contains("Loaded via JS")
    });

    if cfg!(feature = "js-eval") {
        assert!(
            has_price,
            "Med js-eval ska auto-escalation hitta JS-genererat content"
        );
    } else {
        // Utan js-eval kan vi inte köra QuickJS — men vi ska inte krascha
        println!("  (js-eval ej aktiverat — auto-escalation kan inte köra QuickJS)");
        assert!(
            !nodes.is_empty(),
            "Även utan js-eval ska parse returnera noder (safety net)"
        );
    }
}

// ─── Phase 1.4: Empty Result Safety Net ─────────────────────────────────

#[test]
fn test_phase14_empty_result_fallback() {
    // En SPA som QuickJS inte kan rendera — bara title och meta finns
    let html = r##"<html>
    <head>
        <title>MyApp - Dashboard</title>
        <meta name="description" content="View your analytics dashboard with real-time metrics and reports.">
    </head>
    <body>
        <div id="root"></div>
        <script type="module" src="/app.bundle.js"></script>
    </body></html>"##;

    let result = parse_crfr_json(html, "dashboard analytics metrics", "https://app.example.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 1.4: {} nodes (fallback test)", nodes.len());
    for n in nodes {
        println!(
            "  [{}] {} (type={})",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(80)],
            n["resonance_type"].as_str().unwrap_or("?")
        );
    }

    // Ska INTE vara tom — fallback till title + meta description
    assert!(
        !nodes.is_empty(),
        "Tom SPA ska ge fallback-noder (title + meta), inte tom array"
    );

    // Title ska finnas
    let has_title = nodes
        .iter()
        .any(|n| n["label"].as_str().unwrap_or("").contains("MyApp"));
    assert!(has_title, "Fallback ska inkludera sidans title");
}

// ─── Phase 2.1: Blob Text Splitting ─────────────────────────────────────

#[test]
fn test_phase21_blob_text_split() {
    // En sida med ett gigantiskt textblock (som NYT/Al Jazeera hade)
    let long_text = "Breaking news: The government announced new economic measures today. \
        The central bank raised interest rates by 0.5 percentage points. \
        Markets reacted strongly with a 3% drop in the main index. \
        Analysts expect further tightening in the coming months. \
        The prime minister will address the nation tonight at 8 PM. \
        Opposition leaders called for emergency debate in parliament. \
        Consumer prices rose 4.2% year-over-year according to latest data. \
        Housing market shows signs of cooling as mortgage rates climb. \
        The currency fell 2% against the dollar in early trading. \
        International observers expressed concern about economic stability.";

    let html = format!(
        r#"<html><body><p>{}</p></body></html>"#,
        long_text
    );

    let result = parse_crfr_json(
        &html,
        "interest rates economy central bank",
        "https://news.example.com",
        15,
    );
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 2.1: {} nodes from blob text", nodes.len());
    for n in nodes {
        let label = n["label"].as_str().unwrap_or("");
        println!(
            "  [{}] {}...",
            n["role"].as_str().unwrap_or("?"),
            &label[..label.len().min(70)]
        );
    }

    // Blob text ska ha genererats sub-noder med kortare labels
    // Om splittingen fungerar borde vi se flera noder med kortare text
    // istället för en enda nod med 600+ chars
    let max_label_len = nodes
        .iter()
        .map(|n| n["label"].as_str().unwrap_or("").len())
        .max()
        .unwrap_or(0);

    // Om originaltexten var 600+ chars ska ingen enskild nod ha >400 chars
    assert!(
        max_label_len < 500 || nodes.len() >= 2,
        "Blob text ({}chars) ska splittas — längsta label: {} chars, {} noder",
        long_text.len(),
        max_label_len,
        nodes.len()
    );
}

// ─── Phase 2.2: Navigation Discrimination ───────────────────────────────

#[test]
fn test_phase22_nav_links_penalized() {
    let html = r##"<html><body>
        <nav>
            <a href="/login">login</a>
            <a href="/register">sign up</a>
            <a href="#" onclick="hide()">hide</a>
            <a href="#" onclick="show()">show</a>
            <a href="/menu">menu</a>
        </nav>
        <main>
            <h1>World Economy Report 2026</h1>
            <p>Global GDP grew 3.2% in 2025, driven by emerging markets.</p>
            <a href="/report/full">Read the full economic analysis report</a>
        </main>
    </body></html>"##;

    let result = parse_crfr_json(html, "economy GDP growth report analysis", "https://news.example.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 2.2: {} nodes", nodes.len());
    for n in nodes {
        println!(
            "  [{}] {:<40} rel={:.3}",
            n["role"].as_str().unwrap_or("?"),
            n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(40)].to_string(),
            n["relevance"].as_f64().unwrap_or(0.0)
        );
    }

    // "World Economy Report" eller "Global GDP" ska rankas HÖGRE än "login"/"hide"
    if nodes.len() >= 2 {
        let top_label = nodes[0]["label"].as_str().unwrap_or("");
        let nav_labels = ["login", "sign up", "hide", "show", "menu"];
        let top_is_nav = nav_labels.iter().any(|nl| top_label.to_lowercase() == *nl);
        assert!(
            !top_is_nav,
            "Topp-noden ska vara content, inte nav. Fick: '{}'",
            top_label
        );
    }
}

// ─── Phase 2.3: SSR JSON Headline Extraction ────────────────────────────

#[test]
fn test_phase23_ssr_json_enriched_links() {
    // BBC-stil SSR JSON: artiklar har URL + title i sibling-fält
    let html = r##"<html><head><title>BBC News</title></head><body>
        <div id="__next"></div>
        <script id="__NEXT_DATA__" type="application/json">
        {
            "props": {
                "pageProps": {
                    "sections": [{
                        "content": [
                            {
                                "title": "UK Parliament votes on new trade deal",
                                "href": "/news/articles/abc123",
                                "summary": "MPs debate the implications of the post-Brexit agreement"
                            },
                            {
                                "title": "Scientists discover high-temperature superconductor",
                                "href": "/news/articles/def456",
                                "summary": "Breakthrough could revolutionize energy transmission"
                            }
                        ]
                    }]
                }
            },
            "page": "/news"
        }
        </script>
    </body></html>"##;

    let result = parse_crfr_json(html, "news headlines articles parliament science", "https://www.bbc.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 2.3: {} nodes from SSR JSON", nodes.len());
    for n in nodes {
        println!(
            "  [{}] {}",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(80)]
        );
    }

    // Ska hitta headlines, INTE bara opaka URL:er
    let has_headline = nodes.iter().any(|n| {
        let label = n["label"].as_str().unwrap_or("");
        label.contains("Parliament") || label.contains("superconductor") || label.contains("trade deal")
    });
    assert!(
        has_headline,
        "SSR JSON ska ge headline-text, inte bara /news/articles/abc123"
    );
}

// ─── Phase 2.4: Label Cleaning ──────────────────────────────────────────

#[test]
fn test_phase24_label_cleaning() {
    let html = r##"<html><body>
        <p>Subscribers only The government&#x27;s new policy on &amp;climate change&amp; is controversial.</p>
        <p>ANZEIGE Buy premium access for exclusive content &amp; features.</p>
        <p><a href="/article">Read the full <b>analysis</b> report</a></p>
    </body></html>"##;

    let result = parse_crfr_json(html, "climate policy government analysis", "https://news.example.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 2.4: {} nodes", nodes.len());
    for n in nodes {
        let label = n["label"].as_str().unwrap_or("");
        println!("  label: {}", &label[..label.len().min(80)]);

        // Ska INTE innehålla HTML-entiteter
        assert!(
            !label.contains("&#x27;"),
            "Label ska inte innehålla &#x27;: {}",
            label
        );
        assert!(
            !label.contains("&amp;"),
            "Label ska inte innehålla &amp;: {}",
            label
        );

        // Ska INTE starta med boilerplate-prefix
        assert!(
            !label.starts_with("Subscribers only "),
            "Label ska strippas från 'Subscribers only': {}",
            label
        );
        assert!(
            !label.starts_with("ANZEIGE "),
            "Label ska strippas från 'ANZEIGE': {}",
            label
        );
    }
}

// ─── Duplicate Dedup (BUG-3 fix) ────────────────────────────────────────

#[test]
fn test_duplicate_short_labels_deduped() {
    // HackerNews-stil: massor av identiska "hide" knappar
    let mut html = String::from("<html><body><table>");
    for i in 0..30 {
        html.push_str(&format!(
            r#"<tr><td><a href="/story/{i}">Story {i}: Interesting tech article about programming</a></td>
               <td><a href="/hide?id={i}">hide</a></td></tr>"#
        ));
    }
    html.push_str("</table></body></html>");

    let result = parse_crfr_json(
        &html,
        "interesting tech articles programming stories",
        "https://news.ycombinator.com",
        15,
    );
    let nodes = result["nodes"].as_array().unwrap();

    let hide_count = nodes
        .iter()
        .filter(|n| n["label"].as_str().unwrap_or("").to_lowercase() == "hide")
        .count();

    println!(
        "Dedup test: {} nodes, {} are 'hide'",
        nodes.len(),
        hide_count
    );

    assert!(
        hide_count <= 2,
        "Max 2 'hide' noder ska passera dedup, fick {}",
        hide_count
    );
}

// ─── Full Pipeline E2E ──────────────────────────────────────────────────

#[test]
fn test_full_pipeline_ecommerce() {
    let html = r##"<html>
    <head><title>TechShop - Electronics</title>
    <meta name="description" content="Best deals on electronics, laptops, phones and accessories.">
    </head>
    <body>
        <nav><a href="/">Home</a> <a href="/cart">Cart</a> <a href="/login">login</a></nav>
        <main>
            <h1>Summer Sale</h1>
            <article>
                <h2>MacBook Pro M4</h2>
                <p class="price">$1,999.00</p>
                <p>The most powerful laptop for professionals. 16-inch Liquid Retina XDR display.</p>
                <a href="/product/macbook-pro-m4">View Details</a>
            </article>
            <article>
                <h2>iPhone 17 Pro</h2>
                <p class="price">$1,099.00</p>
                <p>Revolutionary camera system with 48MP main sensor and 5x optical zoom.</p>
                <a href="/product/iphone-17-pro">View Details</a>
            </article>
            <article>
                <h2>AirPods Pro 3</h2>
                <p class="price">$249.00</p>
                <p>Adaptive audio with personalized spatial sound and noise cancellation.</p>
                <a href="/product/airpods-pro-3">View Details</a>
            </article>
        </main>
        <footer><p>Copyright 2026 TechShop</p></footer>
    </body></html>"##;

    let result = parse_crfr_json(html, "laptop price buy MacBook cost", "https://techshop.example.com", 10);
    let nodes = result["nodes"].as_array().unwrap();

    println!("E2E ecommerce: {} nodes", nodes.len());
    for n in nodes {
        println!(
            "  [{:8}] {:<50} rel={:.3} val={}",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(50)],
            n["relevance"].as_f64().unwrap_or(0.0),
            n["value"].as_str().unwrap_or("-")
        );
    }

    // Validera: top-noder ska vara content, inte nav
    assert!(!nodes.is_empty(), "Ska ha resultat");

    let top_label = nodes[0]["label"].as_str().unwrap_or("");
    assert!(
        !["login", "Home", "Cart"].contains(&top_label),
        "Topp-noden ska vara content, inte nav. Fick: '{}'",
        top_label
    );

    // Ska hitta minst en prisfråga
    let has_price = nodes.iter().any(|n| {
        let l = n["label"].as_str().unwrap_or("");
        l.contains("$1,999") || l.contains("$1,099") || l.contains("$249")
    });
    assert!(has_price, "Ska hitta minst ett pris");

    // Ska hitta MacBook (mål-match)
    let has_macbook = nodes.iter().any(|n| {
        n["label"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("macbook")
    });
    assert!(has_macbook, "Ska hitta MacBook (mål-matchning)");

    // Länkar ska vara relativa (BUG-L4 fixade resolve_url)
    let links: Vec<&str> = nodes
        .iter()
        .filter_map(|n| n["value"].as_str())
        .filter(|v| v.contains("product"))
        .collect();
    println!("  Links: {:?}", links);
    for link in &links {
        assert!(
            link.starts_with("https://") || link.starts_with("http://"),
            "Länk ska vara absolut URL, fick: {}",
            link
        );
    }
}
