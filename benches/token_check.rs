/// Quick token analysis — what does AetherAgent actually output?
use aether_agent::{embedding, parse_to_semantic_tree, parse_top_nodes};

fn main() {
    // Init embedding
    let model_path = std::env::var("AETHER_EMBEDDING_MODEL")
        .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".to_string());
    let vocab_path =
        std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".to_string());
    if let (Ok(model), Ok(vocab)) = (
        std::fs::read(&model_path),
        std::fs::read_to_string(&vocab_path),
    ) {
        let _ = embedding::init_global(&model, &vocab);
        println!("[Embedding loaded]");
    }

    let html =
        std::fs::read_to_string("benches/campfire_fixture.html").expect("campfire_fixture.html");

    let full = parse_to_semantic_tree(&html, "buy the backpack", "https://shop.com");
    let top5 = parse_top_nodes(&html, "buy the backpack", "https://shop.com", 5);
    let top10 = parse_top_nodes(&html, "buy the backpack", "https://shop.com", 10);

    println!("\n=== Campfire Commerce: What AetherAgent Returns ===");
    println!(
        "Input HTML:       {:>6} chars  (~{} tokens)",
        html.len(),
        html.len() / 4
    );
    println!(
        "Full tree output: {:>6} chars  (~{} tokens)",
        full.len(),
        full.len() / 4
    );
    println!(
        "Top-5 output:     {:>6} chars  (~{} tokens)",
        top5.len(),
        top5.len() / 4
    );
    println!(
        "Top-10 output:    {:>6} chars  (~{} tokens)",
        top10.len(),
        top10.len() / 4
    );
    println!();
    println!(
        "Full/HTML ratio:  {:.1}%",
        full.len() as f64 / html.len() as f64 * 100.0
    );
    println!(
        "Top5/HTML ratio:  {:.1}%",
        top5.len() as f64 / html.len() as f64 * 100.0
    );
    println!(
        "Top10/HTML ratio: {:.1}%",
        top10.len() as f64 / html.len() as f64 * 100.0
    );

    // Count top-level nodes
    let v: serde_json::Value = serde_json::from_str(&full).unwrap();
    let nodes = v["nodes"].as_array().unwrap();
    println!("\nFull tree: {} top-level nodes", nodes.len());

    let v5: serde_json::Value = serde_json::from_str(&top5).unwrap();
    let n5 = v5["nodes"].as_array().unwrap_or_else(|| {
        println!(
            "top5 JSON keys: {:?}",
            v5.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
        println!("top5 first 500 chars: {}", &top5[..top5.len().min(500)]);
        panic!("no nodes array in top5");
    });
    println!("Top-5:     {} nodes", n5.len());

    let v10: serde_json::Value = serde_json::from_str(&top10).unwrap();
    let n10 = if let Some(arr) = v10["nodes"].as_array() {
        arr.clone()
    } else {
        vec![]
    };
    println!("Top-10:    {} nodes\n", n10.len());

    // Show what top-5 contains
    println!("=== Top-5 Most Relevant Nodes ===");
    for n in n5.iter() {
        let label = n["label"].as_str().unwrap_or("?");
        let short = if label.len() > 60 {
            &label[..60]
        } else {
            label
        };
        println!(
            "  role={:<10} rel={:.3} label=\"{}\"",
            n["role"].as_str().unwrap_or("?"),
            n["relevance"].as_f64().unwrap_or(0.0),
            short
        );
    }

    println!("\n=== What an LLM Actually Needs (top-5) ===");
    println!("{top5}");

    // Now test with a fixture
    println!("\n\n=== Fixture 01: Ecommerce Product ===");
    let fix = std::fs::read_to_string("tests/fixtures/01_ecommerce_product.html").unwrap();
    let fix_full = parse_to_semantic_tree(&fix, "buy iPhone add to cart", "https://shop.se");
    let fix_top5 = parse_top_nodes(&fix, "buy iPhone add to cart", "https://shop.se", 5);
    println!(
        "Input HTML:  {:>6} chars (~{} tokens)",
        fix.len(),
        fix.len() / 4
    );
    println!(
        "Full tree:   {:>6} chars (~{} tokens)",
        fix_full.len(),
        fix_full.len() / 4
    );
    println!(
        "Top-5:       {:>6} chars (~{} tokens)",
        fix_top5.len(),
        fix_top5.len() / 4
    );
    println!(
        "Savings top5 vs HTML: {:.1}%",
        (1.0 - fix_top5.len() as f64 / fix.len() as f64) * 100.0
    );
}
