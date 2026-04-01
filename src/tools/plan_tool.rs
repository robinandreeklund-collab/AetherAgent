// Tool 4: plan — Goal decomposition + causal reasoning
//
// Ersätter: compile_goal, build_causal_graph, predict_action_outcome,
//           find_safest_path, execute_plan, fetch_plan

use serde::Deserialize;

use super::{build_tree, now_ms, ToolResult};

/// Request-parametrar för plan-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct PlanRequest {
    /// Mål att bryta ner (krävs)
    pub goal: String,
    /// Åtgärd: "compile" (default), "predict", "safest_path", "execute"
    #[serde(default = "default_action")]
    pub action: String,
    /// Kausal-graf JSON (för predict/safest_path)
    #[serde(default)]
    pub graph_json: Option<String>,
    /// HTML (för execute)
    #[serde(default)]
    pub html: Option<String>,
    /// URL (för execute)
    #[serde(default)]
    pub url: Option<String>,
    /// Max steg i plan
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Redan avslutade steg (för execute)
    #[serde(default)]
    pub completed_steps: Vec<u32>,
    /// Streaming-läge
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_action() -> String {
    "compile".to_string()
}

fn default_max_steps() -> u32 {
    10
}

fn default_true() -> bool {
    true
}

/// Kör plan-verktyget
pub fn execute(req: &PlanRequest) -> ToolResult {
    let start = now_ms();

    match req.action.as_str() {
        "compile" => execute_compile(req, start),
        "predict" => execute_predict(req, start),
        "safest_path" => execute_safest_path(req, start),
        "execute" => execute_plan(req, start),
        other => ToolResult::err(
            format!("Okänd action: '{other}'. Använd 'compile', 'predict', 'safest_path', eller 'execute'."),
            now_ms().saturating_sub(start),
        ),
    }
}

fn execute_compile(req: &PlanRequest, start: u64) -> ToolResult {
    let plan = crate::compiler::compile_goal(&req.goal);
    let data =
        serde_json::to_value(&plan).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

fn execute_predict(req: &PlanRequest, start: u64) -> ToolResult {
    let graph_json = match &req.graph_json {
        Some(g) => g.as_str(),
        None => {
            return ToolResult::err(
                "'graph_json' krävs för action=predict",
                now_ms().saturating_sub(start),
            )
        }
    };

    let graph = match crate::causal::CausalGraph::from_json(graph_json) {
        Ok(g) => g,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig graph_json: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    let outcome = graph.predict_outcome(&req.goal, None);
    let data = serde_json::to_value(&outcome)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

fn execute_safest_path(req: &PlanRequest, start: u64) -> ToolResult {
    let graph_json = match &req.graph_json {
        Some(g) => g.as_str(),
        None => {
            return ToolResult::err(
                "'graph_json' krävs för action=safest_path",
                now_ms().saturating_sub(start),
            )
        }
    };

    let graph = match crate::causal::CausalGraph::from_json(graph_json) {
        Ok(g) => g,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig graph_json: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    let path = graph.find_safest_path(&req.goal, req.max_steps);
    let data =
        serde_json::to_value(&path).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

fn execute_plan(req: &PlanRequest, start: u64) -> ToolResult {
    let html = match &req.html {
        Some(h) => h.as_str(),
        None => {
            return ToolResult::err(
                "'html' krävs för action=execute",
                now_ms().saturating_sub(start),
            )
        }
    };
    let url = req.url.as_deref().unwrap_or("");

    let plan = crate::compiler::compile_goal(&req.goal);
    let tree = build_tree(html, &req.goal, url);
    let result = crate::compiler::execute_plan(&plan, &tree, &req.completed_steps);

    let data = serde_json::to_value(&result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_compile() {
        let req = PlanRequest {
            goal: "köp billigaste flyget till London".to_string(),
            action: "compile".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Compile ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(
            data["sub_goals"].is_array(),
            "Ska innehålla sub_goals-array"
        );
        let steps = data["sub_goals"].as_array().unwrap();
        assert!(!steps.is_empty(), "Ska generera minst ett steg");
    }

    #[test]
    fn test_plan_compile_default_action() {
        let req = PlanRequest {
            goal: "logga in på sidan".to_string(),
            action: "compile".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Default compile ska lyckas");
    }

    #[test]
    fn test_plan_predict_no_graph() {
        let req = PlanRequest {
            goal: "test".to_string(),
            action: "predict".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Predict utan graf ska ge fel");
    }

    #[test]
    fn test_plan_safest_path_no_graph() {
        let req = PlanRequest {
            goal: "test".to_string(),
            action: "safest_path".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Safest path utan graf ska ge fel");
    }

    #[test]
    fn test_plan_execute_with_html() {
        let html = r##"<html><body>
        <h1>Sök flyg</h1>
        <input placeholder="Från">
        <input placeholder="Till">
        <button>Sök</button>
        </body></html>"##;

        let req = PlanRequest {
            goal: "sök flyg Stockholm till London".to_string(),
            action: "execute".to_string(),
            graph_json: None,
            html: Some(html.to_string()),
            url: Some("https://flyg.se".to_string()),
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Execute ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_plan_execute_no_html() {
        let req = PlanRequest {
            goal: "test".to_string(),
            action: "execute".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Execute utan HTML ska ge fel");
    }

    #[test]
    fn test_plan_unknown_action() {
        let req = PlanRequest {
            goal: "test".to_string(),
            action: "fly".to_string(),
            graph_json: None,
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänd action ska ge fel");
    }

    #[test]
    fn test_plan_with_causal_graph() {
        // Bygg en enkel graf
        let mut graph = crate::causal::CausalGraph::new();
        let s0 = graph.add_state("https://flyg.se", 10, 0, vec!["sök".to_string()]);
        let s1 = graph.add_state(
            "https://flyg.se/results",
            50,
            0,
            vec!["resultat".to_string(), "pris".to_string()],
        );
        graph.add_edge(s0, s1, "click_search", crate::compiler::ActionType::Click);

        let graph_json = graph.to_json();

        let req = PlanRequest {
            goal: "hitta flygresultat".to_string(),
            action: "safest_path".to_string(),
            graph_json: Some(graph_json),
            html: None,
            url: None,
            max_steps: 10,
            completed_steps: vec![],
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Safest path med graf ska lyckas: {:?}",
            result.error
        );
    }
}
