//! CRFR Benchmark — measures Causal Resonance Field Retrieval performance
//! across synthetic scenarios of varying complexity.
//!
//! Run with: `cargo run --bin aether-crfr-bench`

use std::time::Instant;

use aether_agent::resonance::{ResonanceField, ResonanceResult};
use aether_agent::types::{NodeState, SemanticNode, TrustLevel};

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Skapa en testnod med angivna egenskaper
fn make_node(id: u32, role: &str, label: &str, children: Vec<SemanticNode>) -> SemanticNode {
    SemanticNode {
        id,
        role: role.to_string(),
        label: label.to_string(),
        value: None,
        state: NodeState::default_state(),
        action: None,
        relevance: 0.0,
        trust: TrustLevel::Untrusted,
        children,
        html_id: None,
        name: None,
        bbox: None,
    }
}

/// Enkel textlikhet-baseline (ordöverlapp) — används för jämförelse mot CRFR
fn baseline_text_similarity(query: &str, candidate: &str) -> f32 {
    let query_lower = query.to_lowercase();
    let cand_lower = candidate.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    let cand_words: Vec<&str> = cand_lower.split_whitespace().collect();

    if query_words.is_empty() || cand_words.is_empty() {
        return 0.0;
    }

    let matches = query_words
        .iter()
        .filter(|qw| {
            cand_words
                .iter()
                .any(|cw| cw.contains(*qw) || qw.contains(cw))
        })
        .count();

    matches as f32 / query_words.len() as f32
}

/// Samla alla noder i ett platt index (rekursivt)
fn flatten_nodes(nodes: &[SemanticNode]) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    fn collect(node: &SemanticNode, out: &mut Vec<(u32, String)>) {
        out.push((node.id, node.label.clone()));
        for child in &node.children {
            collect(child, out);
        }
    }
    for n in nodes {
        collect(n, &mut result);
    }
    result
}

// ─── Scenariogeneratorer ────────────────────────────────────────────────────

/// E-handelsscenario med 50 noder: nav, produktkort, knappar, sidfot
fn build_ecommerce_scenario() -> Vec<SemanticNode> {
    let mut id = 1u32;
    let mut next_id = || {
        let current = id;
        id += 1;
        current
    };

    // Navigering
    let nav_items: Vec<SemanticNode> = ["Home", "Laptops", "Phones", "Tablets", "Deals"]
        .iter()
        .map(|label| make_node(next_id(), "link", label, vec![]))
        .collect();
    let nav = make_node(next_id(), "navigation", "Main Navigation", nav_items);

    // Produktkort — 8 stycken med pris, namn, knapp
    let mut products = Vec::new();
    let product_data = [
        ("Gaming Laptop X1", "$1299", "Add to cart"),
        ("Budget Laptop A5", "$549", "Add to cart"),
        ("Ultrabook Pro 15", "$1899", "Add to cart"),
        ("Student Laptop SE", "$399", "Add to cart"),
        ("Workstation W7", "$2499", "Add to cart"),
        ("Chromebook C3", "$249", "Add to cart"),
        ("Laptop Sleeve Case", "$29", "Add to cart"),
        ("USB-C Hub Adapter", "$45", "Add to cart"),
    ];
    for (name, price, action_label) in &product_data {
        let title = make_node(next_id(), "heading", name, vec![]);
        let price_node = make_node(next_id(), "text", &format!("Price: {price}"), vec![]);
        let button = make_node(next_id(), "button", action_label, vec![]);
        let card = make_node(next_id(), "group", name, vec![title, price_node, button]);
        products.push(card);
    }
    let product_grid = make_node(next_id(), "region", "Products", products);

    // Sidfot
    let footer_links: Vec<SemanticNode> = ["About", "Contact", "Privacy", "Terms"]
        .iter()
        .map(|l| make_node(next_id(), "link", l, vec![]))
        .collect();
    let footer = make_node(next_id(), "contentinfo", "Footer", footer_links);

    vec![nav, product_grid, footer]
}

/// Nyhetsartikelscenario med ~200 noder
fn build_news_scenario() -> Vec<SemanticNode> {
    let mut id = 1u32;
    let mut next_id = || {
        let current = id;
        id += 1;
        current
    };

    // Sidhuvud med logotyp och nav
    let logo = make_node(next_id(), "img", "NewsDaily Logo", vec![]);
    let nav_items: Vec<SemanticNode> =
        ["World", "Politics", "Science", "Climate", "Tech", "Sports"]
            .iter()
            .map(|l| make_node(next_id(), "link", l, vec![]))
            .collect();
    let nav = make_node(next_id(), "navigation", "Main Menu", nav_items);
    let header = make_node(next_id(), "banner", "Site Header", vec![logo, nav]);

    // Artikelkropp — rubrik, författare, stycken
    let title = make_node(
        next_id(),
        "heading",
        "Climate Change Effects on Arctic Ice Sheets Accelerating",
        vec![],
    );
    let author = make_node(
        next_id(),
        "text",
        "By Dr. Sarah Johnson, Climate Correspondent",
        vec![],
    );
    let author_contact = make_node(next_id(), "link", "sarah.johnson@newsdaily.com", vec![]);

    // 30 stycken med varierande innehåll
    let paragraph_texts = [
        "New research shows accelerating ice loss in the Arctic region.",
        "Scientists have measured record temperatures across Greenland.",
        "The effects of climate change are becoming more visible each year.",
        "Rising sea levels threaten coastal communities worldwide.",
        "Carbon emissions continue to rise despite international agreements.",
        "Renewable energy adoption is increasing but not fast enough.",
        "Arctic wildlife is facing unprecedented habitat loss.",
        "Permafrost thawing releases stored methane into the atmosphere.",
        "Ocean acidification affects marine ecosystems globally.",
        "Global temperature has risen 1.2 degrees since pre-industrial times.",
    ];
    let mut paragraphs: Vec<SemanticNode> = paragraph_texts
        .iter()
        .map(|t| make_node(next_id(), "text", t, vec![]))
        .collect();

    // Ytterligare stycken för att nå ~200 noder
    for i in 0..20 {
        let text = format!("Additional analysis paragraph {}", i + 1);
        paragraphs.push(make_node(next_id(), "text", &text, vec![]));
    }

    let mut article_children = vec![title, author, author_contact];
    article_children.extend(paragraphs);
    let article = make_node(next_id(), "article", "Main Article", article_children);

    // Sidopanel — relaterade artiklar, annonser
    let mut sidebar_items = Vec::new();
    for i in 0..10 {
        let link = make_node(
            next_id(),
            "link",
            &format!("Related Article {}", i + 1),
            vec![],
        );
        sidebar_items.push(link);
    }
    let related = make_node(next_id(), "region", "Related Articles", sidebar_items);

    let mut ads = Vec::new();
    for i in 0..5 {
        let ad = make_node(
            next_id(),
            "img",
            &format!("Advertisement {}", i + 1),
            vec![],
        );
        ads.push(ad);
    }
    let ad_section = make_node(next_id(), "complementary", "Advertisements", ads);
    let sidebar = make_node(
        next_id(),
        "complementary",
        "Sidebar",
        vec![related, ad_section],
    );

    // Kommentarsfält — 40 kommentarer
    let mut comments = Vec::new();
    for i in 0..40 {
        let username = make_node(next_id(), "text", &format!("User{}", i + 1), vec![]);
        let comment_text = make_node(
            next_id(),
            "text",
            &format!("Comment about the article #{}", i + 1),
            vec![],
        );
        let reply_btn = make_node(next_id(), "button", "Reply", vec![]);
        let comment = make_node(
            next_id(),
            "group",
            &format!("Comment {}", i + 1),
            vec![username, comment_text, reply_btn],
        );
        comments.push(comment);
    }
    let comments_section = make_node(next_id(), "region", "Comments", comments);

    // Sidfot
    let footer_items: Vec<SemanticNode> = ["Contact Us", "About", "Privacy Policy", "Careers"]
        .iter()
        .map(|l| make_node(next_id(), "link", l, vec![]))
        .collect();
    let footer = make_node(next_id(), "contentinfo", "Footer", footer_items);

    vec![header, article, sidebar, comments_section, footer]
}

/// Sökresultatscenario med 100 noder
fn build_search_scenario() -> Vec<SemanticNode> {
    let mut id = 1u32;
    let mut next_id = || {
        let current = id;
        id += 1;
        current
    };

    // Sökfält
    let search_input = make_node(next_id(), "searchbox", "Search the web", vec![]);
    let search_btn = make_node(next_id(), "button", "Search", vec![]);
    let search_bar = make_node(
        next_id(),
        "search",
        "Search Form",
        vec![search_input, search_btn],
    );

    // 10 sökresultat med titel, beskrivning och URL
    let result_data = [
        (
            "The Rust Programming Language Book",
            "Official tutorial and reference for learning Rust",
            "doc.rust-lang.org/book",
        ),
        (
            "Rust by Example - Interactive Tutorial",
            "Learn Rust through annotated example programs",
            "doc.rust-lang.org/rust-by-example",
        ),
        (
            "Rust Programming Tutorial for Beginners",
            "Complete beginner guide to Rust programming language",
            "example.com/rust-tutorial",
        ),
        (
            "Advanced Rust Programming Patterns",
            "Deep dive into Rust ownership, lifetimes, and traits",
            "example.com/advanced-rust",
        ),
        (
            "Rust vs Go: Which to Choose in 2025",
            "Comparison of Rust and Go for systems programming",
            "blog.example.com/rust-vs-go",
        ),
        (
            "Building Web APIs with Rust",
            "Tutorial on creating REST APIs using Actix and Rocket",
            "example.com/rust-web-apis",
        ),
        (
            "Rust WASM Tutorial - WebAssembly Guide",
            "How to compile Rust to WebAssembly step by step",
            "example.com/rust-wasm",
        ),
        (
            "Async Programming in Rust",
            "Understanding async/await and Tokio runtime",
            "example.com/async-rust",
        ),
        (
            "Python Tutorial for Data Science",
            "Learn Python for machine learning and data analysis",
            "example.com/python-ml",
        ),
        (
            "JavaScript Frameworks Comparison 2025",
            "React vs Vue vs Svelte performance benchmarks",
            "example.com/js-frameworks",
        ),
    ];

    let mut results = Vec::new();
    for (title, snippet, url) in &result_data {
        let title_node = make_node(next_id(), "link", title, vec![]);
        let snippet_node = make_node(next_id(), "text", snippet, vec![]);
        let url_node = make_node(next_id(), "text", url, vec![]);
        let result = make_node(
            next_id(),
            "group",
            title,
            vec![title_node, snippet_node, url_node],
        );
        results.push(result);
    }

    // Paginering
    let mut page_links = Vec::new();
    for i in 1..=10 {
        page_links.push(make_node(next_id(), "link", &format!("Page {i}"), vec![]));
    }
    let pagination = make_node(next_id(), "navigation", "Pagination", page_links);

    // Sidofält — filter
    let filters: Vec<SemanticNode> = ["All", "Images", "Videos", "News", "Books"]
        .iter()
        .map(|l| make_node(next_id(), "link", l, vec![]))
        .collect();
    let filter_nav = make_node(next_id(), "navigation", "Search Filters", filters);

    let results_region = make_node(next_id(), "region", "Search Results", results);

    vec![search_bar, filter_nav, results_region, pagination]
}

/// Stor SPA med 500 noder och djup nästning
fn build_large_spa_scenario() -> Vec<SemanticNode> {
    let mut id = 1u32;
    let mut next_id = || {
        let current = id;
        id += 1;
        current
    };

    // Djupt nästade strukturella wrappers
    fn build_nested(
        depth: usize,
        breadth: usize,
        next_id: &mut impl FnMut() -> u32,
        labels: &[&str],
        label_idx: &mut usize,
    ) -> SemanticNode {
        if depth == 0 {
            let label = labels[*label_idx % labels.len()];
            *label_idx += 1;
            return make_node(next_id(), "text", label, vec![]);
        }
        let children: Vec<SemanticNode> = (0..breadth)
            .map(|_| build_nested(depth - 1, breadth, next_id, labels, label_idx))
            .collect();
        make_node(
            next_id(),
            "group",
            &format!("Wrapper depth={depth}"),
            children,
        )
    }

    let spa_labels = [
        "Dashboard",
        "User Profile",
        "Account Settings",
        "Notification Bell",
        "Search Users",
        "Messages",
        "Logout",
        "Theme Toggle",
        "Language Selector",
        "Help Center",
        "User Avatar",
        "Status Indicator",
        "Recent Activity",
        "Quick Actions",
        "Settings Gear",
    ];
    let mut label_idx = 0;

    // Sidhuvud
    let logo = make_node(next_id(), "img", "App Logo", vec![]);
    let nav_items: Vec<SemanticNode> = ["Dashboard", "Users", "Settings", "Reports"]
        .iter()
        .map(|l| make_node(next_id(), "link", l, vec![]))
        .collect();
    let nav = make_node(next_id(), "navigation", "App Navigation", nav_items);
    let user_menu = make_node(next_id(), "button", "User Account Settings", vec![]);
    let header = make_node(
        next_id(),
        "banner",
        "App Header",
        vec![logo, nav, user_menu],
    );

    // Huvudinnehåll — djupt nästade sektioner
    let section1 = build_nested(4, 3, &mut next_id, &spa_labels, &mut label_idx);
    let section2 = build_nested(4, 3, &mut next_id, &spa_labels, &mut label_idx);
    let section3 = build_nested(3, 4, &mut next_id, &spa_labels, &mut label_idx);
    let section4 = build_nested(3, 4, &mut next_id, &spa_labels, &mut label_idx);
    let main = make_node(
        next_id(),
        "main",
        "Main Content",
        vec![section1, section2, section3, section4],
    );

    // Sidfot
    let footer_links: Vec<SemanticNode> = ["Terms", "Privacy", "Help", "Status"]
        .iter()
        .map(|l| make_node(next_id(), "link", l, vec![]))
        .collect();
    let footer = make_node(next_id(), "contentinfo", "Footer", footer_links);

    vec![header, main, footer]
}

// ─── Benchmark-körning ──────────────────────────────────────────────────────

/// Resultat från en benchmark-körning
struct BenchResult {
    scenario_name: String,
    total_nodes: usize,
    build_ms: f64,
    propagation_ms: f64,
    resonant_count: usize,
    cache_hit_ms: f64,
    cache_resonant_count: usize,
    token_savings_pct: f64,
    baseline_recall: usize,
    crfr_recall: usize,
}

/// Kör baseline-scoring: räkna hur många noder som har ordöverlapp > 0
fn baseline_recall(nodes: &[SemanticNode], goal: &str) -> usize {
    let flat = flatten_nodes(nodes);
    flat.iter()
        .filter(|(_, label)| baseline_text_similarity(goal, label) > 0.15)
        .count()
}

/// Kör en fullständig benchmark för ett scenario med ett mål
fn run_benchmark(scenario_name: &str, nodes: &[SemanticNode], goal: &str) -> BenchResult {
    let total = flatten_nodes(nodes).len();

    // Mät fältbygge
    let t0 = Instant::now();
    let mut field = ResonanceField::from_semantic_tree(nodes, "https://bench.test");
    let build_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // Mät propagering
    let t1 = Instant::now();
    let result: ResonanceResult = field.propagate(goal);
    let propagation_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let resonant_count = result.nodes_resonant;

    // Mät cache-träff
    let t2 = Instant::now();
    let cached_result = field.propagate(goal);
    let cache_hit_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let cache_resonant_count = cached_result.nodes_resonant;

    // Token-besparing
    let token_savings_pct = if total > 0 {
        (1.0 - resonant_count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    // Baseline-jämförelse
    let bl_recall = baseline_recall(nodes, goal);

    BenchResult {
        scenario_name: scenario_name.to_string(),
        total_nodes: total,
        build_ms,
        propagation_ms,
        resonant_count,
        cache_hit_ms,
        cache_resonant_count,
        token_savings_pct,
        baseline_recall: bl_recall,
        crfr_recall: resonant_count,
    }
}

/// Kör kausal inlärningstest: upprepa queries med feedback
fn run_causal_learning_test(
    nodes: &[SemanticNode],
    goal: &str,
    feedback_node_ids: &[u32],
    iterations: usize,
) {
    let mut field = ResonanceField::from_semantic_tree(nodes, "https://bench.test");

    println!("  Causal learning ({iterations} iterations):");

    for i in 0..iterations {
        let result = field.propagate(goal);
        let count = result.nodes_resonant;
        let top_score = result.hits.first().map(|h| h.amplitude).unwrap_or(0.0);

        println!(
            "    Iteration {}: {} resonant nodes, top score: {:.3}",
            i + 1,
            count,
            top_score
        );

        // Ge feedback på utvalda noder
        field.feedback(goal, feedback_node_ids);
    }
}

/// Kör multi-mål-interferenstest
fn run_multi_goal_test(nodes: &[SemanticNode], goals: &[&str]) {
    let mut field = ResonanceField::from_semantic_tree(nodes, "https://bench.test");

    println!("  Multi-goal interference:");
    let mut previous_ids: Vec<u32> = Vec::new();

    for goal in goals {
        let t = Instant::now();
        let result = field.propagate(goal);
        let ms = t.elapsed().as_secs_f64() * 1000.0;

        // Beräkna överlapp med föregående resultat
        let current_ids: Vec<u32> = result.hits.iter().map(|h| h.node_id).collect();
        let overlap = if previous_ids.is_empty() {
            0
        } else {
            current_ids
                .iter()
                .filter(|id| previous_ids.contains(id))
                .count()
        };

        println!(
            "    Goal \"{goal}\": {} resonant ({:.3} ms), overlap with prev: {overlap}",
            result.nodes_resonant, ms
        );

        previous_ids = current_ids;
    }
}

// ─── Formatering ────────────────────────────────────────────────────────────

/// Skriv ut strukturerade resultat
fn print_result(r: &BenchResult) {
    println!("Scenario: {} ({} nodes)", r.scenario_name, r.total_nodes);
    println!("  Field build:     {:.2} ms", r.build_ms);
    println!(
        "  Propagation:     {:.2} ms ({} resonant nodes)",
        r.propagation_ms, r.resonant_count
    );
    println!(
        "  Cache-hit query: {:.4} ms ({} resonant nodes)",
        r.cache_hit_ms, r.cache_resonant_count
    );
    println!(
        "  Token savings:   {:.1}% ({}/{} nodes emitted)",
        r.token_savings_pct, r.resonant_count, r.total_nodes
    );

    // Beräkna recall-förhållande
    let recall_ratio = if r.baseline_recall > 0 {
        r.crfr_recall as f64 / r.baseline_recall as f64
    } else {
        f64::INFINITY
    };
    println!(
        "  vs Baseline:     {:.1}x node coverage (CRFR: {}, baseline: {})",
        recall_ratio, r.crfr_recall, r.baseline_recall
    );
    println!();
}

// ─── Huvudprogram ───────────────────────────────────────────────────────────

fn main() {
    println!("=== CRFR Benchmark ===");
    println!();

    // --- Scenario 1: E-handel ---
    let ecom_nodes = build_ecommerce_scenario();
    let r1 = run_benchmark("E-commerce", &ecom_nodes, "find cheapest laptop price");
    print_result(&r1);

    let r1b = run_benchmark("E-commerce (add to cart)", &ecom_nodes, "add to cart");
    print_result(&r1b);

    // --- Scenario 2: Nyhetsartikel ---
    let news_nodes = build_news_scenario();
    let r2 = run_benchmark("News article", &news_nodes, "climate change effects");
    print_result(&r2);

    let r2b = run_benchmark(
        "News (author contact)",
        &news_nodes,
        "author contact information",
    );
    print_result(&r2b);

    // --- Scenario 3: Sökresultat ---
    let search_nodes = build_search_scenario();
    let r3 = run_benchmark("Search results", &search_nodes, "rust programming tutorial");
    print_result(&r3);

    // --- Scenario 4: Stor SPA ---
    let spa_nodes = build_large_spa_scenario();
    let r4 = run_benchmark("Large SPA", &spa_nodes, "user account settings");
    print_result(&r4);

    // --- Scenario 5: Kausal inlärning ---
    println!("--- Causal Learning ---");
    println!();

    // E-handelsscenario: ge feedback på produktkortnoder
    // Nod-ID 9 = Budget Laptop, 10 = dess pris, 11 = Add to cart-knapp
    println!("Scenario: E-commerce causal learning");
    run_causal_learning_test(&ecom_nodes, "find cheapest laptop price", &[9, 10], 5);
    println!();

    // Nyhetsscenario: ge feedback på artikelinnehåll
    println!("Scenario: News causal learning");
    run_causal_learning_test(&news_nodes, "climate change effects", &[11, 12, 13], 5);
    println!();

    // --- Multi-mål-interferens ---
    println!("--- Multi-goal Interference ---");
    println!();

    println!("Scenario: E-commerce");
    run_multi_goal_test(&ecom_nodes, &["find cheapest laptop price", "add to cart"]);
    println!();

    println!("Scenario: News article");
    run_multi_goal_test(
        &news_nodes,
        &["climate change effects", "author contact information"],
    );
    println!();

    println!("Scenario: Search results");
    run_multi_goal_test(
        &search_nodes,
        &["rust programming tutorial", "python data science"],
    );
    println!();

    // --- Sammanfattning ---
    println!("=== Summary ===");
    println!();
    let all_results = [&r1, &r1b, &r2, &r2b, &r3, &r4];
    let avg_build: f64 =
        all_results.iter().map(|r| r.build_ms).sum::<f64>() / all_results.len() as f64;
    let avg_prop: f64 =
        all_results.iter().map(|r| r.propagation_ms).sum::<f64>() / all_results.len() as f64;
    let avg_savings: f64 =
        all_results.iter().map(|r| r.token_savings_pct).sum::<f64>() / all_results.len() as f64;
    let avg_cache: f64 =
        all_results.iter().map(|r| r.cache_hit_ms).sum::<f64>() / all_results.len() as f64;

    println!("Avg field build:     {avg_build:.2} ms");
    println!("Avg propagation:     {avg_prop:.2} ms");
    println!("Avg cache-hit:       {avg_cache:.4} ms");
    println!("Avg token savings:   {avg_savings:.1}%");
    println!();
    println!("=== Done ===");
}
