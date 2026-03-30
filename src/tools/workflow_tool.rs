// Tool 11: workflow — Workflow orchestration
//
// Ersätter: alla 8 orchestrator-endpoints + workflow memory

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för workflow-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowRequest {
    /// Åtgärd (krävs): "create", "page", "report", "complete", "rollback", "status"
    pub action: String,
    /// Orchestrator JSON-state
    #[serde(default)]
    pub workflow_json: Option<String>,
    /// Mål (för create)
    #[serde(default)]
    pub goal: Option<String>,
    /// Start-URL (för create)
    #[serde(default)]
    pub start_url: Option<String>,
    /// Konfiguration JSON (för create)
    #[serde(default)]
    pub config_json: Option<String>,
    /// HTML (för page)
    #[serde(default)]
    pub html: Option<String>,
    /// URL (för page)
    #[serde(default)]
    pub url: Option<String>,
    /// Resultat JSON (för report — click/fill/extract)
    #[serde(default)]
    pub result_json: Option<String>,
    /// Typ av rapport: "click", "fill", "extract"
    #[serde(default)]
    pub report_type: Option<String>,
    /// Steg-index (för complete/rollback)
    #[serde(default)]
    pub step_index: Option<u32>,
}

/// Kör workflow-verktyget
pub fn execute(req: &WorkflowRequest) -> ToolResult {
    let start = now_ms();

    match req.action.as_str() {
        "create" => execute_create(req, start),
        "page" => execute_page(req, start),
        "report" => execute_report(req, start),
        "complete" => execute_complete(req, start),
        "rollback" => execute_rollback(req, start),
        "status" => execute_status(req, start),
        other => ToolResult::err(
            format!("Okänd action: '{other}'. Använd: create, page, report, complete, rollback, status."),
            now_ms() - start,
        ),
    }
}

fn parse_orchestrator(
    json: &Option<String>,
    start: u64,
) -> Result<crate::orchestrator::WorkflowOrchestrator, ToolResult> {
    match json {
        Some(j) => crate::orchestrator::WorkflowOrchestrator::from_json(j)
            .map_err(|e| ToolResult::err(format!("Ogiltig workflow_json: {e}"), now_ms() - start)),
        None => Err(ToolResult::err("'workflow_json' krävs", now_ms() - start)),
    }
}

fn execute_create(req: &WorkflowRequest, start: u64) -> ToolResult {
    let goal = match &req.goal {
        Some(g) => g.as_str(),
        None => return ToolResult::err("'goal' krävs för action=create", now_ms() - start),
    };
    let start_url = match &req.start_url {
        Some(u) => u.as_str(),
        None => return ToolResult::err("'start_url' krävs för action=create", now_ms() - start),
    };

    let config = match &req.config_json {
        Some(c) => serde_json::from_str(c).unwrap_or_default(),
        None => crate::orchestrator::OrchestratorConfig::default(),
    };

    let mut orchestrator = crate::orchestrator::WorkflowOrchestrator::new(goal, start_url, config);
    let step_result = orchestrator.start(now_ms());

    let data = serde_json::json!({
        "workflow_json": orchestrator.to_json(),
        "step_result": serde_json::to_value(&step_result).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_page(req: &WorkflowRequest, start: u64) -> ToolResult {
    let mut orchestrator = match parse_orchestrator(&req.workflow_json, start) {
        Ok(o) => o,
        Err(e) => return e,
    };
    let html = match &req.html {
        Some(h) => h.as_str(),
        None => return ToolResult::err("'html' krävs för action=page", now_ms() - start),
    };
    let url = match &req.url {
        Some(u) => u.as_str(),
        None => return ToolResult::err("'url' krävs för action=page", now_ms() - start),
    };

    let step_result = orchestrator.provide_page(html, url, now_ms());

    let data = serde_json::json!({
        "workflow_json": orchestrator.to_json(),
        "step_result": serde_json::to_value(&step_result).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_report(req: &WorkflowRequest, start: u64) -> ToolResult {
    let mut orchestrator = match parse_orchestrator(&req.workflow_json, start) {
        Ok(o) => o,
        Err(e) => return e,
    };
    let result_json = match &req.result_json {
        Some(r) => r.as_str(),
        None => return ToolResult::err("'result_json' krävs för action=report", now_ms() - start),
    };
    let report_type = match &req.report_type {
        Some(t) => t.as_str(),
        None => return ToolResult::err("'report_type' krävs för action=report", now_ms() - start),
    };

    let step_result = match report_type {
        "click" => {
            let click: crate::types::ClickResult = match serde_json::from_str(result_json) {
                Ok(r) => r,
                Err(e) => {
                    return ToolResult::err(
                        format!("Ogiltig click result_json: {e}"),
                        now_ms() - start,
                    )
                }
            };
            orchestrator.report_click_result(&click, now_ms())
        }
        "fill" => {
            let fill: crate::types::FillFormResult = match serde_json::from_str(result_json) {
                Ok(r) => r,
                Err(e) => {
                    return ToolResult::err(
                        format!("Ogiltig fill result_json: {e}"),
                        now_ms() - start,
                    )
                }
            };
            orchestrator.report_fill_result(&fill, now_ms())
        }
        "extract" => {
            let extract: crate::types::ExtractDataResult = match serde_json::from_str(result_json) {
                Ok(r) => r,
                Err(e) => {
                    return ToolResult::err(
                        format!("Ogiltig extract result_json: {e}"),
                        now_ms() - start,
                    )
                }
            };
            orchestrator.report_extract_result(&extract, now_ms())
        }
        other => {
            return ToolResult::err(
                format!("Okänd report_type: '{other}'. Använd 'click', 'fill', eller 'extract'."),
                now_ms() - start,
            )
        }
    };

    let data = serde_json::json!({
        "workflow_json": orchestrator.to_json(),
        "step_result": serde_json::to_value(&step_result).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_complete(req: &WorkflowRequest, start: u64) -> ToolResult {
    let mut orchestrator = match parse_orchestrator(&req.workflow_json, start) {
        Ok(o) => o,
        Err(e) => return e,
    };
    let step_index = match req.step_index {
        Some(i) => i,
        None => return ToolResult::err("'step_index' krävs för action=complete", now_ms() - start),
    };

    let step_result = orchestrator.report_step_completed(step_index, now_ms());

    let data = serde_json::json!({
        "workflow_json": orchestrator.to_json(),
        "step_result": serde_json::to_value(&step_result).unwrap_or_default(),
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_rollback(req: &WorkflowRequest, start: u64) -> ToolResult {
    let mut orchestrator = match parse_orchestrator(&req.workflow_json, start) {
        Ok(o) => o,
        Err(e) => return e,
    };
    let step_index = match req.step_index {
        Some(i) => i,
        None => return ToolResult::err("'step_index' krävs för action=rollback", now_ms() - start),
    };

    orchestrator.rollback_step(step_index);

    let data = serde_json::json!({
        "workflow_json": orchestrator.to_json(),
        "rolled_back": step_index,
    });
    ToolResult::ok(data, now_ms() - start)
}

fn execute_status(req: &WorkflowRequest, start: u64) -> ToolResult {
    let orchestrator = match parse_orchestrator(&req.workflow_json, start) {
        Ok(o) => o,
        Err(e) => return e,
    };

    let status_json = orchestrator.to_json();
    let status: serde_json::Value =
        serde_json::from_str(&status_json).unwrap_or(serde_json::json!({}));

    ToolResult::ok(status, now_ms() - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_create() {
        let req = WorkflowRequest {
            action: "create".to_string(),
            workflow_json: None,
            goal: Some("köp biljett".to_string()),
            start_url: Some("https://sj.se".to_string()),
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Create ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(
            data["workflow_json"].is_string(),
            "Ska returnera workflow_json"
        );
    }

    #[test]
    fn test_workflow_create_no_goal() {
        let req = WorkflowRequest {
            action: "create".to_string(),
            workflow_json: None,
            goal: None,
            start_url: Some("https://sj.se".to_string()),
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Create utan goal ska ge fel");
    }

    #[test]
    fn test_workflow_status() {
        // Skapa workflow först
        let create_req = WorkflowRequest {
            action: "create".to_string(),
            workflow_json: None,
            goal: Some("test".to_string()),
            start_url: Some("https://test.se".to_string()),
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let create_result = execute(&create_req);
        let wf_json = create_result.data.unwrap()["workflow_json"]
            .as_str()
            .unwrap()
            .to_string();

        // Kolla status
        let status_req = WorkflowRequest {
            action: "status".to_string(),
            workflow_json: Some(wf_json),
            goal: None,
            start_url: None,
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&status_req);
        assert!(
            result.error.is_none(),
            "Status ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_workflow_page_no_html() {
        let create_req = WorkflowRequest {
            action: "create".to_string(),
            workflow_json: None,
            goal: Some("test".to_string()),
            start_url: Some("https://test.se".to_string()),
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let wf_json = execute(&create_req).data.unwrap()["workflow_json"]
            .as_str()
            .unwrap()
            .to_string();

        let page_req = WorkflowRequest {
            action: "page".to_string(),
            workflow_json: Some(wf_json),
            goal: None,
            start_url: None,
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&page_req);
        assert!(result.error.is_some(), "Page utan html ska ge fel");
    }

    #[test]
    fn test_workflow_unknown_action() {
        let req = WorkflowRequest {
            action: "fly".to_string(),
            workflow_json: None,
            goal: None,
            start_url: None,
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänd action ska ge fel");
    }

    #[test]
    fn test_workflow_complete_no_index() {
        let create_req = WorkflowRequest {
            action: "create".to_string(),
            workflow_json: None,
            goal: Some("test".to_string()),
            start_url: Some("https://test.se".to_string()),
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let wf_json = execute(&create_req).data.unwrap()["workflow_json"]
            .as_str()
            .unwrap()
            .to_string();

        let req = WorkflowRequest {
            action: "complete".to_string(),
            workflow_json: Some(wf_json),
            goal: None,
            start_url: None,
            config_json: None,
            html: None,
            url: None,
            result_json: None,
            report_type: None,
            step_index: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_some(),
            "Complete utan step_index ska ge fel"
        );
    }
}
