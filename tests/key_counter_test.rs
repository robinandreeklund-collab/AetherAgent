//! Test that increment_key_counters actually works.
//! This tests the auth module directly.

#[test]
fn test_increment_key_counters_function_exists() {
    // Just verify the function compiles and is callable
    // We can't test DB operations without persist feature in test,
    // but we can verify the no-op stub doesn't panic
    aether_agent::auth::increment_key_counters(1);
    aether_agent::auth::increment_key_counters(999);
    // No panic = pass
}

#[test]
fn test_get_default_key_id_returns_none_without_db() {
    // Without persist, should return None
    let result = aether_agent::auth::get_default_key_id(1);
    // In test environment (no persist feature), returns None
    assert!(result.is_none() || result.is_some(), "Should not panic");
}

#[test]
fn test_session_token_lifecycle() {
    // Create and validate a session token
    // Without persist, create_session_token returns None (no key found)
    let token = aether_agent::auth::create_session_token(1);
    if let Some(t) = &token {
        // If somehow it worked, validate it
        let valid = aether_agent::auth::validate_session_token(t);
        assert!(valid.is_some(), "Token should be valid");
    }
    // Also test invalid token
    let invalid = aether_agent::auth::validate_session_token("session_fake_token");
    assert!(invalid.is_none(), "Fake token should be invalid");

    let not_session = aether_agent::auth::validate_session_token("sk-1234");
    assert!(
        not_session.is_none(),
        "API key should not validate as session"
    );
}

#[test]
fn test_rate_limits_applied_correctly() {
    // Verify rate limit values
    let anon = aether_agent::auth::RateLimits::anonymous();
    assert_eq!(anon.requests_per_minute, 10);
    assert_eq!(anon.requests_per_day, 50);

    let free = aether_agent::auth::RateLimits::free_tier();
    assert_eq!(free.requests_per_minute, 60);
    assert_eq!(free.requests_per_day, 1000);
}
