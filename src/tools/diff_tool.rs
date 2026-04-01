// Tool 5: diff — Semantic tree diffing
//
// Ersätter: diff_trees, diff_semantic_trees

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för diff-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct DiffRequest {
    /// Tidigare träd (JSON)
    #[serde(default)]
    pub old_tree: Option<String>,
    /// Nytt träd (JSON)
    #[serde(default)]
    pub new_tree: Option<String>,
}

/// Kör diff-verktyget
pub fn execute(req: &DiffRequest) -> ToolResult {
    let start = now_ms();

    let old_json = match &req.old_tree {
        Some(t) => t.as_str(),
        None => return ToolResult::err("'old_tree' krävs", now_ms().saturating_sub(start)),
    };

    let new_json = match &req.new_tree {
        Some(t) => t.as_str(),
        None => return ToolResult::err("'new_tree' krävs", now_ms().saturating_sub(start)),
    };

    let old_tree: crate::types::SemanticTree = match serde_json::from_str(old_json) {
        Ok(t) => t,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig old_tree JSON: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    let new_tree: crate::types::SemanticTree = match serde_json::from_str(new_json) {
        Ok(t) => t,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig new_tree JSON: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    let delta = crate::diff::diff_trees(&old_tree, &new_tree);
    let data = serde_json::to_value(&delta)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms().saturating_sub(start))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree_json(url: &str, nodes_json: &str) -> String {
        format!(
            r#"{{"url":"{url}","title":"Test","goal":"test","nodes":{nodes_json},"injection_warnings":[],"parse_time_ms":0}}"#
        )
    }

    #[test]
    fn test_diff_basic() {
        let old = make_tree_json(
            "https://a.se",
            r#"[{"id":1,"role":"button","label":"Köp","relevance":0.9,"trust":"Untrusted","children":[],"state":{"disabled":false,"checked":false,"expanded":false,"focused":false,"visible":true}}]"#,
        );
        let new = make_tree_json(
            "https://a.se",
            r#"[{"id":1,"role":"button","label":"Köpt!","relevance":0.9,"trust":"Untrusted","children":[],"state":{"disabled":true,"checked":false,"expanded":false,"focused":false,"visible":true}}]"#,
        );

        let req = DiffRequest {
            old_tree: Some(old),
            new_tree: Some(new),
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Diff ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        let changes = data["changes"].as_array();
        assert!(
            changes.map(|c| !c.is_empty()).unwrap_or(false),
            "Ska hitta ändringar"
        );
    }

    #[test]
    fn test_diff_no_old() {
        let req = DiffRequest {
            old_tree: None,
            new_tree: Some("{}".to_string()),
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ska ge fel utan old_tree");
    }

    #[test]
    fn test_diff_no_new() {
        let req = DiffRequest {
            old_tree: Some("{}".to_string()),
            new_tree: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ska ge fel utan new_tree");
    }

    #[test]
    fn test_diff_invalid_json() {
        let req = DiffRequest {
            old_tree: Some("not json".to_string()),
            new_tree: Some("{}".to_string()),
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ska ge fel med ogiltig JSON");
    }

    #[test]
    fn test_diff_identical_trees() {
        let tree = make_tree_json(
            "https://a.se",
            r#"[{"id":1,"role":"text","label":"Hej","relevance":0.5,"trust":"Untrusted","children":[],"state":{"disabled":false,"checked":false,"expanded":false,"focused":false,"visible":true}}]"#,
        );
        let req = DiffRequest {
            old_tree: Some(tree.clone()),
            new_tree: Some(tree),
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Identiska träd ska lyckas");
        let data = result.data.unwrap();
        let empty = vec![];
        let changes = data["changes"].as_array().unwrap_or(&empty);
        assert!(changes.is_empty(), "Identiska träd ska inte ha ändringar");
    }
}
