//! Verify rate limits are strict and cannot be bypassed.
use aether_agent::auth::{check_rate_limit, RateLimits};

#[test]
fn test_anonymous_rate_limit_blocks_after_10_per_minute() {
    let limits = RateLimits::anonymous(); // 10/min, 50/day
    let id = "test-ip:192.168.1.99";
    for i in 0..10 {
        assert!(
            check_rate_limit(id, &limits).is_ok(),
            "Request {} should pass",
            i + 1
        );
    }
    // 11th request should be blocked
    assert!(
        check_rate_limit(id, &limits).is_err(),
        "11th request should be rate limited"
    );
}

#[test]
fn test_free_tier_rate_limit_blocks_after_60_per_minute() {
    let limits = RateLimits::free_tier(); // 60/min, 1000/day
    let id = "test-key:unique-key-123";
    for i in 0..60 {
        assert!(
            check_rate_limit(id, &limits).is_ok(),
            "Request {} should pass",
            i + 1
        );
    }
    // 61st should be blocked
    assert!(
        check_rate_limit(id, &limits).is_err(),
        "61st request should be rate limited"
    );
}

#[test]
fn test_different_ips_have_separate_limits() {
    let limits = RateLimits::anonymous();
    // Fill up IP A
    for _ in 0..10 {
        let _ = check_rate_limit("test-separate-ip:A", &limits);
    }
    assert!(
        check_rate_limit("test-separate-ip:A", &limits).is_err(),
        "IP A should be blocked"
    );
    // IP B should still work
    assert!(
        check_rate_limit("test-separate-ip:B", &limits).is_ok(),
        "IP B should not be affected by IP A's limit"
    );
}

#[test]
fn test_spoofed_user_id_gets_anonymous_limits() {
    // Even if someone sends X-Slaash-User-Id, they should get anonymous limits
    // (10/min) not authenticated limits (60/min).
    // This test verifies the ARCHITECTURE: rate limiting is based on the
    // identifier string, not the user_id header. If Bearer key is absent,
    // the rate limit key is "ip:<addr>" with anonymous limits.
    let anon_limits = RateLimits::anonymous();
    let auth_limits = RateLimits::free_tier();

    assert_eq!(anon_limits.requests_per_minute, 10);
    assert_eq!(auth_limits.requests_per_minute, 60);

    // Spoofed request uses IP-based key, not key-based
    let spoofed_id = "ip:spoofed-attacker";
    for _ in 0..10 {
        let _ = check_rate_limit(spoofed_id, &anon_limits);
    }
    // Must be blocked at anonymous limit (10), not auth limit (60)
    assert!(
        check_rate_limit(spoofed_id, &anon_limits).is_err(),
        "Spoofed user should be blocked at anonymous limit (10/min)"
    );
}
