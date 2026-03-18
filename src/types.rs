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
    #[serde(default = "NodeState::default_state")]
    pub state: NodeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub relevance: f32,
    pub trust: TrustLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SemanticNode>,
    /// Originalt HTML id-attribut, för selector hints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_id: Option<String>,
    /// Originalt HTML name-attribut, för formulärmatchning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Bounding box (Fas 9c: Multimodal Grounding)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BoundingBox>,
}

/// Bounding box – spatial position för multimodal grounding (Fas 9c)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoundingBox {
    /// X-koordinat (vänster kant, pixlar)
    pub x: f32,
    /// Y-koordinat (övre kant, pixlar)
    pub y: f32,
    /// Bredd (pixlar)
    pub width: f32,
    /// Höjd (pixlar)
    pub height: f32,
}

/// Elementets interaktionstillstånd
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeState {
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub focused: bool,
    #[serde(default = "default_visible", skip_serializing_if = "is_true")]
    pub visible: bool,
}

fn default_visible() -> bool {
    true
}

impl NodeState {
    pub fn default_state() -> Self {
        NodeState {
            disabled: false,
            checked: None,
            expanded: None,
            focused: false,
            visible: true,
        }
    }
}

fn is_false(v: &bool) -> bool {
    !v
}

fn is_true(v: &bool) -> bool {
    *v
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
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
    /// Antal XHR-anrop som fångades och hämtades (Fas 10)
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub xhr_intercepted: u32,
    /// Antal XHR-anrop som blockerades av Semantic Firewall (Fas 10)
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub xhr_blocked: u32,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default)]
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

// ─── HTTP Fetch Types (Fas 7) ────────────────────────────────────────────────

/// Konfiguration för HTTP-fetch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchConfig {
    /// User-Agent-sträng
    #[serde(default = "FetchConfig::default_user_agent")]
    pub user_agent: String,
    /// Timeout i millisekunder
    #[serde(default = "FetchConfig::default_timeout_ms")]
    pub timeout_ms: u64,
    /// Max antal redirects att följa
    #[serde(default = "FetchConfig::default_max_redirects")]
    pub max_redirects: u32,
    /// Respektera robots.txt (Googles parser)
    #[serde(default)]
    pub respect_robots_txt: bool,
    /// Rate limit: max requests per sekund per domän
    #[serde(default = "FetchConfig::default_rate_limit_rps")]
    pub rate_limit_rps: u32,
    /// Extra headers (key → value)
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    /// Aktivera XHR-interception (Fas 10, default false)
    #[serde(default)]
    pub intercept_xhr: bool,
    /// Max antal XHR-anrop att följa (Fas 10)
    #[serde(default = "FetchConfig::default_intercept_max")]
    pub intercept_max: usize,
    /// Timeout per XHR-anrop i ms (Fas 10)
    #[serde(default = "FetchConfig::default_intercept_timeout_ms")]
    pub intercept_timeout_ms: u64,
}

/// Resultat av en HTTP-fetch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    /// Slutgiltig URL (efter redirects)
    pub final_url: String,
    /// HTTP-statuskod
    pub status_code: u16,
    /// Content-Type-header
    pub content_type: String,
    /// HTML-body (om text/html)
    pub body: String,
    /// Kedja av redirects [url1 → url2 → ...]
    pub redirect_chain: Vec<String>,
    /// Fetch-tid i millisekunder
    pub fetch_time_ms: u64,
    /// Responsens storlek i bytes
    pub body_size_bytes: usize,
}

/// Kombinerat fetch + parse-resultat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchAndParseResult {
    /// Fetch-metadata
    pub fetch: FetchResult,
    /// Semantiskt träd (samma som /api/parse)
    pub tree: SemanticTree,
    /// Total tid (fetch + parse) i millisekunder
    pub total_time_ms: u64,
}

/// Kombinerat fetch + click-resultat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchAndClickResult {
    /// Fetch-metadata
    pub fetch: FetchResult,
    /// Click-resultat (samma som /api/click)
    pub click: ClickResult,
    pub total_time_ms: u64,
}

/// Kombinerat fetch + extract-resultat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchAndExtractResult {
    /// Fetch-metadata
    pub fetch: FetchResult,
    /// Extraherad data
    pub extract: ExtractDataResult,
    pub total_time_ms: u64,
}

/// Kombinerat fetch + full pipeline (compile + parse + execute)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchAndPlanResult {
    /// Fetch-metadata
    pub fetch: FetchResult,
    /// Kompilerad plan
    pub plan_json: String,
    /// Exekveringsresultat
    pub execution_json: String,
    pub total_time_ms: u64,
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
            bbox: None,
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
        Self::default()
    }
}

impl FetchConfig {
    fn default_user_agent() -> String {
        "AetherAgent/0.1 (LLM-native browser engine)".to_string()
    }

    fn default_timeout_ms() -> u64 {
        10_000
    }

    fn default_max_redirects() -> u32 {
        10
    }

    fn default_rate_limit_rps() -> u32 {
        2
    }

    fn default_intercept_max() -> usize {
        10
    }

    fn default_intercept_timeout_ms() -> u64 {
        2000
    }
}

impl Default for FetchConfig {
    fn default() -> Self {
        FetchConfig {
            user_agent: Self::default_user_agent(),
            timeout_ms: Self::default_timeout_ms(),
            max_redirects: Self::default_max_redirects(),
            respect_robots_txt: false,
            rate_limit_rps: Self::default_rate_limit_rps(),
            extra_headers: HashMap::new(),
            intercept_xhr: false,
            intercept_max: Self::default_intercept_max(),
            intercept_timeout_ms: Self::default_intercept_timeout_ms(),
        }
    }
}
