/// UTF-8 boundary panic reproduction
/// The exact crash from production logs:
///   byte index 60 is not a char boundary; it is inside 'à' (bytes 59..61) of
///   `Notation des allocataires : la CAF étend sa surveillance à l'analyse...`
use aether_agent::*;

#[test]
fn test_french_utf8_label_does_not_panic() {
    // This label caused the production crash — 'à' at byte 59-61, slice at byte 60
    let html = r##"<html><body>
        <article>
            <h2>Notation des allocataires : la CAF étend sa surveillance à l'analyse des revenus en temps réel</h2>
            <p>Les allocataires sont désormais surveillés par un algorithme de scoring.</p>
        </article>
    </body></html>"##;

    // First parse to populate CRFR field
    let result1 = parse_crfr(
        html,
        "CAF surveillance allocataires notation",
        "https://utf8-crash-test.example.com",
        5,
        false,
        "json",
    );
    let p1: serde_json::Value = serde_json::from_str(&result1).expect("First parse should work");
    assert!(
        p1["node_count"].as_u64().unwrap_or(0) > 0,
        "Should have nodes"
    );

    // Give feedback to create causal memory
    let nodes = p1["nodes"].as_array().unwrap();
    if let Some(id) = nodes.first().and_then(|n| n["id"].as_u64()) {
        let ids_json = format!("[{}]", id);
        crfr_feedback(
            "https://utf8-crash-test.example.com",
            "CAF surveillance allocataires notation",
            &ids_json,
        );
    }

    // Second parse with slightly different HTML triggers migrate_learning_from
    // which calls transfer_from with the French label — this is where the crash was
    let html2 = r##"<html><body>
        <article>
            <h2>Notation des allocataires : la CAF étend sa surveillance à l'analyse des revenus en temps réel</h2>
            <p>Mise à jour: Les allocataires sont désormais surveillés plus strictement.</p>
        </article>
    </body></html>"##;

    let result2 = parse_crfr(
        html2,
        "CAF surveillance allocataires notation",
        "https://utf8-crash-test.example.com",
        5,
        false,
        "json",
    );
    let p2: serde_json::Value = serde_json::from_str(&result2)
        .expect("Second parse with French UTF-8 should NOT panic at byte boundary");
    println!("UTF-8 test: nodes={}, no panic", p2["node_count"]);
}

#[test]
fn test_japanese_utf8_label_does_not_panic() {
    let html = r##"<html><body>
        <h1>朝日新聞デジタル：朝日新聞社のニュースサイト - 速報・政治・経済・国際</h1>
        <p>最新のニュースをお届けします。政治、経済、国際、社会、スポーツ、文化など。</p>
    </body></html>"##;

    let result = parse_crfr(
        html,
        "ニュース 政治 経済 速報",
        "https://utf8-jp-test.example.com",
        5,
        false,
        "json",
    );
    let p: serde_json::Value =
        serde_json::from_str(&result).expect("Japanese UTF-8 should not panic");
    println!("Japanese UTF-8: nodes={}", p["node_count"]);
}

#[test]
fn test_mixed_multibyte_utf8_does_not_panic() {
    // Swedish ö/ä/å, German ü/ß, Emoji, CJK
    let html = r##"<html><body>
        <p>Größe der Änderung: über 50% Verbesserung für Ärzte in München</p>
        <p>Köpenhamn → Göteborg → Malmö: Räksmörgås och gräddfil</p>
        <p>🚀 Launch: AI-driven analytics for 日本語テスト and 中文测试</p>
    </body></html>"##;

    let result = parse_crfr(
        html,
        "Änderung Verbesserung Göteborg analytics",
        "https://utf8-mixed-test.example.com",
        5,
        false,
        "json",
    );
    let p: serde_json::Value =
        serde_json::from_str(&result).expect("Mixed multibyte UTF-8 should not panic");
    println!("Mixed UTF-8: nodes={}", p["node_count"]);
}
