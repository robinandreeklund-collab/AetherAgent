//! Verify per-user feedback works end-to-end.
//! Tests the full chain: parse_crfr → crfr_feedback_user → verify isolation.

#[test]
fn test_user_feedback_returns_ok() {
    let html = r##"<html><body>
        <h1>Rust Programming</h1>
        <p id="p1">Rust was created by Graydon Hoare at Mozilla in 2010</p>
        <p id="p2">It is a systems programming language focused on safety</p>
        <p id="p3">Rust prevents memory bugs without garbage collection</p>
    </body></html>"##;

    // Step 1: Parse with CRFR to create a cached field
    let result = aether_agent::parse_crfr(
        html,
        "when was Rust created and by whom",
        "https://test-feedback-user.example.com/rust",
        10,
        false,
        "json",
    );
    assert!(
        result.contains("nodes"),
        "parse_crfr should return nodes, got: {}",
        &result[..200.min(result.len())]
    );

    // Step 2: Send per-user feedback with user_id=42
    let feedback = aether_agent::crfr_feedback_user(
        "https://test-feedback-user.example.com/rust",
        "when was Rust created and by whom",
        "[1, 2]",
        42,
    );
    eprintln!("Feedback response: {}", feedback);
    assert!(
        feedback.contains("\"status\":\"ok\""),
        "Per-user feedback should return ok, got: {}",
        feedback
    );
    assert!(
        feedback.contains("\"isolated\":true"),
        "Should be isolated per user, got: {}",
        feedback
    );
}

#[test]
fn test_user_feedback_rejected_without_user_id() {
    let result = aether_agent::crfr_feedback_user(
        "https://example.com",
        "test",
        "[1]",
        0, // user_id=0 should be rejected
    );
    assert!(
        result.contains("error") || result.contains("required"),
        "user_id=0 should be rejected, got: {}",
        result
    );
}

#[test]
fn test_user_feedback_does_not_affect_global() {
    let html = r##"<html><body>
        <h1>Global vs User Test</h1>
        <p>Important paragraph about testing</p>
    </body></html>"##;

    let url = "https://test-global-vs-user.example.com/page";

    // Parse to create global field
    let _ = aether_agent::parse_crfr(html, "testing", url, 10, false, "json");

    // Send global feedback (old function — should still exist internally)
    let global_before = aether_agent::crfr_feedback(url, "testing", "[1]");
    eprintln!("Global feedback: {}", global_before);

    // Send user feedback for user 99
    let user_fb = aether_agent::crfr_feedback_user(url, "testing", "[1, 2]", 99);
    eprintln!("User feedback: {}", user_fb);

    // Global feedback should still work (field not consumed by user feedback)
    // Re-parse to ensure field exists
    let _ = aether_agent::parse_crfr(html, "testing", url, 10, false, "json");
    let global_after = aether_agent::crfr_feedback(url, "testing", "[1]");
    eprintln!("Global feedback after user: {}", global_after);
    assert!(
        global_after.contains("\"status\":\"ok\""),
        "Global feedback should still work after user feedback, got: {}",
        global_after
    );
}
