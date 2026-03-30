// Tool 12: collab — Multi-agent collaboration + observability
//
// Ersätter: alla 4 collab-endpoints + tier_stats + memory/cache stats

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för collab-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct CollabRequest {
    /// Åtgärd (krävs): "create", "register", "publish", "fetch", "stats"
    pub action: String,
    /// Store JSON-state
    #[serde(default)]
    pub store_json: Option<String>,
    /// Agent-ID
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Agentens mål
    #[serde(default)]
    pub goal: Option<String>,
    /// URL för delta
    #[serde(default)]
    pub url: Option<String>,
    /// Semantisk delta JSON
    #[serde(default)]
    pub delta_json: Option<String>,
}

/// Kör collab-verktyget
pub fn execute(req: &CollabRequest) -> ToolResult {
    let start = now_ms();

    match req.action.as_str() {
        "create" => {
            let store = crate::collab::SharedDiffStore::new();
            let data = serde_json::json!({
                "store_json": store.to_json(),
                "status": "created",
            });
            ToolResult::ok(data, now_ms() - start)
        }
        "register" => execute_register(req, start),
        "publish" => execute_publish(req, start),
        "fetch" => execute_fetch(req, start),
        "stats" => execute_stats(req, start),
        other => ToolResult::err(
            format!("Okänd action: '{other}'. Använd: create, register, publish, fetch, stats."),
            now_ms() - start,
        ),
    }
}

fn parse_store(
    json: &Option<String>,
    start: u64,
) -> Result<crate::collab::SharedDiffStore, ToolResult> {
    match json {
        Some(j) => crate::collab::SharedDiffStore::from_json(j)
            .map_err(|e| ToolResult::err(format!("Ogiltig store_json: {e}"), now_ms() - start)),
        None => Err(ToolResult::err("'store_json' krävs", now_ms() - start)),
    }
}

fn execute_register(req: &CollabRequest, start: u64) -> ToolResult {
    let mut store = match parse_store(&req.store_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let agent_id = match &req.agent_id {
        Some(id) => id.as_str(),
        None => return ToolResult::err("'agent_id' krävs för action=register", now_ms() - start),
    };
    let goal = req.goal.as_deref().unwrap_or("");

    store.register_agent(agent_id, goal, now_ms());

    let data = serde_json::json!({
        "store_json": store.to_json(),
        "registered": agent_id,
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_publish(req: &CollabRequest, start: u64) -> ToolResult {
    let mut store = match parse_store(&req.store_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let agent_id = match &req.agent_id {
        Some(id) => id.as_str(),
        None => return ToolResult::err("'agent_id' krävs för action=publish", now_ms() - start),
    };
    let url = match &req.url {
        Some(u) => u.as_str(),
        None => return ToolResult::err("'url' krävs för action=publish", now_ms() - start),
    };
    let delta_json = match &req.delta_json {
        Some(d) => d.as_str(),
        None => return ToolResult::err("'delta_json' krävs för action=publish", now_ms() - start),
    };

    let delta: crate::types::SemanticDelta = match serde_json::from_str(delta_json) {
        Ok(d) => d,
        Err(e) => return ToolResult::err(format!("Ogiltig delta_json: {e}"), now_ms() - start),
    };

    store.publish_delta(agent_id, url, delta, now_ms());

    let data = serde_json::json!({
        "store_json": store.to_json(),
        "published": true,
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_fetch(req: &CollabRequest, start: u64) -> ToolResult {
    let mut store = match parse_store(&req.store_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let agent_id = match &req.agent_id {
        Some(id) => id.as_str(),
        None => return ToolResult::err("'agent_id' krävs för action=fetch", now_ms() - start),
    };

    // Automatisk cleanup
    store.cleanup_inactive(now_ms(), 300_000); // 5 min max age

    let result = store.fetch_deltas(agent_id);

    let data = serde_json::json!({
        "store_json": store.to_json(),
        "deltas": serde_json::to_value(&result).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_stats(req: &CollabRequest, start: u64) -> ToolResult {
    // Collab stats
    let collab_stats = if let Some(ref json) = req.store_json {
        match crate::collab::SharedDiffStore::from_json(json) {
            Ok(store) => Some(store.stats()),
            Err(_) => None,
        }
    } else {
        None
    };

    // Tier stats
    let tier_stats = crate::vision_backend::TieredBackend::new(false).stats();

    let data = serde_json::json!({
        "collab": collab_stats.map(|s| serde_json::to_value(s).unwrap_or_default()),
        "tier_stats": serde_json::to_value(&tier_stats).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collab_create() {
        let req = CollabRequest {
            action: "create".to_string(),
            store_json: None,
            agent_id: None,
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Create ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(data["store_json"].is_string(), "Ska returnera store_json");
    }

    #[test]
    fn test_collab_register() {
        let store = crate::collab::SharedDiffStore::new();
        let req = CollabRequest {
            action: "register".to_string(),
            store_json: Some(store.to_json()),
            agent_id: Some("agent_a".to_string()),
            goal: Some("hitta priser".to_string()),
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Register ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert_eq!(data["registered"], "agent_a");
    }

    #[test]
    fn test_collab_register_no_agent_id() {
        let store = crate::collab::SharedDiffStore::new();
        let req = CollabRequest {
            action: "register".to_string(),
            store_json: Some(store.to_json()),
            agent_id: None,
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Register utan agent_id ska ge fel");
    }

    #[test]
    fn test_collab_fetch() {
        let mut store = crate::collab::SharedDiffStore::new();
        store.register_agent("agent_a", "test", now_ms());
        let req = CollabRequest {
            action: "fetch".to_string(),
            store_json: Some(store.to_json()),
            agent_id: Some("agent_a".to_string()),
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Fetch ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_collab_stats() {
        let req = CollabRequest {
            action: "stats".to_string(),
            store_json: None,
            agent_id: None,
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Stats ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(data["tier_stats"].is_object(), "Ska returnera tier_stats");
    }

    #[test]
    fn test_collab_unknown_action() {
        let req = CollabRequest {
            action: "dance".to_string(),
            store_json: None,
            agent_id: None,
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänd action ska ge fel");
    }

    #[test]
    fn test_collab_full_flow() {
        // Create
        let create_req = CollabRequest {
            action: "create".to_string(),
            store_json: None,
            agent_id: None,
            goal: None,
            url: None,
            delta_json: None,
        };
        let store_json = execute(&create_req).data.unwrap()["store_json"]
            .as_str()
            .unwrap()
            .to_string();

        // Register
        let reg_req = CollabRequest {
            action: "register".to_string(),
            store_json: Some(store_json.clone()),
            agent_id: Some("agent_1".to_string()),
            goal: Some("test".to_string()),
            url: None,
            delta_json: None,
        };
        let store_json = execute(&reg_req).data.unwrap()["store_json"]
            .as_str()
            .unwrap()
            .to_string();

        // Fetch (should return empty deltas)
        let fetch_req = CollabRequest {
            action: "fetch".to_string(),
            store_json: Some(store_json),
            agent_id: Some("agent_1".to_string()),
            goal: None,
            url: None,
            delta_json: None,
        };
        let result = execute(&fetch_req);
        assert!(
            result.error.is_none(),
            "Full flow ska lyckas: {:?}",
            result.error
        );
    }
}
