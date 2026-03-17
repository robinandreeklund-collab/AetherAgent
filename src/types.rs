use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Semantic Tree Types ─────────────────────────────────────────────────────

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
    /// Originalt HTML id-attribut, för selector hints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_id: Option<String>,
    /// Originalt HTML name-attribut, för formulärmatchning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Elementets interaktionstillstånd
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

// ─── Intent API Types (Fas 2) ────────────────────────────────────────────────

/// Resultat från find_and_click
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickResult {
    pub found: bool,
    pub node_id: u32,
    pub role: String,
    pub label: String,
    pub action: String,
    pub relevance: f32,
    pub selector_hint: String,
    pub trust: TrustLevel,
    pub injection_warnings: Vec<InjectionWarning>,
    pub parse_time_ms: u64,
}

/// Mappning av ett formulärfält
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldMapping {
    pub field_label: String,
    pub field_role: String,
    pub node_id: u32,
    pub matched_key: String,
    pub value: String,
    pub selector_hint: String,
    pub confidence: f32,
}

/// Resultat från fill_form
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillFormResult {
    pub mappings: Vec<FormFieldMapping>,
    pub unmapped_keys: Vec<String>,
    pub unmapped_fields: Vec<String>,
    pub injection_warnings: Vec<InjectionWarning>,
    pub parse_time_ms: u64,
}

/// En extraherad datapost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedEntry {
    pub key: String,
    pub value: String,
    pub source_node_id: u32,
    pub confidence: f32,
}

/// Resultat från extract_data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractDataResult {
    pub entries: Vec<ExtractedEntry>,
    pub missing_keys: Vec<String>,
    pub injection_warnings: Vec<InjectionWarning>,
    pub parse_time_ms: u64,
}

// ─── Workflow Memory Types (Fas 2) ───────────────────────────────────────────

/// In-memory kontext mellan agent-steg (stateless över WASM-gränsen)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMemory {
    pub steps: Vec<WorkflowStep>,
    pub context: HashMap<String, String>,
}

/// Ett steg i agentens workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub step_index: u32,
    pub action: String,
    pub url: String,
    pub goal: String,
    pub summary: String,
    pub timestamp_ms: u64,
}

// ─── Semantic Diff Types (Fas 4a) ────────────────────────────────────────

/// Typ av förändring i en nod
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    /// Nod lades till (finns i new, inte i old)
    Added,
    /// Nod togs bort (finns i old, inte i new)
    Removed,
    /// Nodens egenskaper förändrades
    Modified,
}

/// En enskild fältförändring i en nod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub before: String,
    pub after: String,
}

/// En förändring i det semantiska trädet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeChange {
    pub node_id: u32,
    pub change_type: ChangeType,
    pub role: String,
    pub label: String,
    pub changes: Vec<FieldChange>,
}

/// Resultatet av en semantic diff mellan två träd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDelta {
    pub url: String,
    pub goal: String,
    pub total_nodes_before: u32,
    pub total_nodes_after: u32,
    pub changes: Vec<NodeChange>,
    /// Hur mycket token-besparing denna delta ger (0.0–1.0)
    pub token_savings_ratio: f32,
    /// Sammanfattning av förändringarna
    pub summary: String,
    pub diff_time_ms: u64,
}

// ─── Implementations ─────────────────────────────────────────────────────────

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
            html_id: None,
            name: None,
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

impl ClickResult {
    /// Tomt resultat när inget element hittades
    pub fn not_found(warnings: Vec<InjectionWarning>, parse_time_ms: u64) -> Self {
        ClickResult {
            found: false,
            node_id: 0,
            role: String::new(),
            label: String::new(),
            action: String::new(),
            relevance: 0.0,
            selector_hint: String::new(),
            trust: TrustLevel::Untrusted,
            injection_warnings: warnings,
            parse_time_ms,
        }
    }
}

impl WorkflowMemory {
    pub fn new() -> Self {
        WorkflowMemory {
            steps: vec![],
            context: HashMap::new(),
        }
    }
}
