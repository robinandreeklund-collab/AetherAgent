use serde::{Deserialize, Serialize};

/// En nod i det semantiska trädet – det LLM:en faktiskt ser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticNode {
    pub id: u32,
    pub role: String,
    pub label: String,
    pub value: Option<String>,
    pub state: NodeState,
    pub action: Option<String>,
    pub relevance: f32,
    pub trust: TrustLevel,
    pub children: Vec<SemanticNode>,
}

/// Elementets interaktionstillstånd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    pub disabled: bool,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub focused: bool,
    pub visible: bool,
}

/// Säkerhetsnivå för innehåll – kärnan i trust shield
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrustLevel {
    /// Innehåll från sidan – behandla alltid som opålitligt
    Untrusted,
    /// Strukturellt element (knapp, länk, formulär) – halvpålitligt
    Structural,
    /// Explicit ARIA-märkt av sidägaren
    Annotated,
}

/// Hela det semantiska trädet – vad som skickas till LLM:en
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticTree {
    pub url: String,
    pub title: String,
    pub goal: String,
    pub nodes: Vec<SemanticNode>,
    pub injection_warnings: Vec<InjectionWarning>,
    pub parse_time_ms: u64,
}

/// Varning när trust shield hittar misstänkt innehåll
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionWarning {
    pub node_id: u32,
    pub reason: String,
    pub severity: WarningSeverity,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarningSeverity {
    Low,
    Medium,
    High,
}

impl SemanticNode {
    pub fn new(id: u32, role: &str, label: &str) -> Self {
        SemanticNode {
            id,
            role: role.to_string(),
            label: label.to_string(),
            value: None,
            state: NodeState {
                disabled: false,
                checked: None,
                expanded: None,
                focused: false,
                visible: true,
            },
            action: None,
            relevance: 0.0,
            trust: TrustLevel::Untrusted,
            children: vec![],
        }
    }

    /// Beräkna action baserat på roll
    pub fn infer_action(role: &str) -> Option<String> {
        match role {
            "button" | "link" | "menuitem" => Some("click".to_string()),
            "textbox" | "searchbox" | "textarea" => Some("type".to_string()),
            "checkbox" | "radio" => Some("toggle".to_string()),
            "combobox" | "listbox" | "select" => Some("select".to_string()),
            "slider" => Some("slide".to_string()),
            _ => None,
        }
    }

    /// Rollens prioritet för goal-relevance scoring
    pub fn role_priority(role: &str) -> f32 {
        match role {
            "button" => 0.9,
            "link" => 0.8,
            "textbox" | "searchbox" => 0.85,
            "checkbox" | "radio" => 0.75,
            "combobox" | "select" => 0.75,
            "heading" => 0.6,
            "img" => 0.4,
            "text" | "paragraph" => 0.3,
            _ => 0.2,
        }
    }
}
