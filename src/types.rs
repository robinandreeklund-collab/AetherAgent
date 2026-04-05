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
    #[serde(default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TrustLevel {
    /// Innehåll från sidan – behandla alltid som opålitligt
    #[default]
    Untrusted,
    /// Strukturellt element (knapp, länk, formulär) – halvpålitligt
    Structural,
    /// Explicit ARIA-märkt av sidägaren
    Annotated,
}

/// Hela det semantiska trädet – vad som skickas till LLM:en
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// URLs som JS anropade via fetch() men inte kunde hämtas i sandbox (BUGG J).
    /// Async-lagret (server/MCP) kan fetcha dessa och merge-a noder i trädet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_fetch_urls: Vec<String>,
    /// Cookies satta av JS under evaluation (för re-fetch vid bot challenge)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub js_cookies: String,
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
    /// Cookies to inject as Cookie header (name=value; name2=value2)
    #[serde(default)]
    pub cookies: String,
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
    /// Om redirects gick till en annan domän (potentiell OAuth/tracking)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cross_domain_redirect: bool,
    /// Set-Cookie headers from HTTP response (for JS cookie bridge)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set_cookie_headers: Vec<String>,
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
}

impl Default for SemanticNode {
    fn default() -> Self {
        Self::new(0, "", "")
    }
}

impl SemanticNode {
    /// Grund klon utan barn — undviker rekursiv deep-clone av hela subträdet.
    /// Används i stream_engine där barn alltid rensas direkt efter clone.
    pub fn clone_shallow(&self) -> Self {
        SemanticNode {
            id: self.id,
            role: self.role.clone(),
            label: self.label.clone(),
            value: self.value.clone(),
            state: self.state.clone(),
            action: self.action.clone(),
            relevance: self.relevance,
            trust: self.trust.clone(),
            children: Vec::new(),
            html_id: self.html_id.clone(),
            name: self.name.clone(),
            bbox: self.bbox.clone(),
        }
    }

    /// Beräkna action baserat på roll
    #[inline]
    pub fn infer_action(role: &str) -> Option<String> {
        match role {
            "button" | "link" | "menuitem" | "cta" | "tab" => Some("click".to_string()),
            "textbox" | "searchbox" | "textarea" => Some("type".to_string()),
            "checkbox" | "radio" | "switch" => Some("toggle".to_string()),
            "combobox" | "listbox" | "select" | "menu" => Some("select".to_string()),
            "slider" => Some("slide".to_string()),
            "product_card" | "dialog" | "video" => Some("click".to_string()),
            "navigation" | "complementary" => Some("click".to_string()),
            _ => None,
        }
    }

    /// Rollens prioritet för goal-relevance scoring
    #[inline]
    pub fn role_priority(role: &str) -> f32 {
        match role {
            "cta" => 0.95,
            "button" => 0.9,
            "textbox" | "searchbox" => 0.85,
            "link" | "tab" => 0.8,
            "checkbox" | "radio" | "switch" => 0.75,
            "combobox" | "select" | "menu" => 0.75,
            "product_card" => 0.7,
            "dialog" => 0.7,
            "price" => 0.65,
            "navigation" | "complementary" => 0.6,
            "heading" => 0.6,
            "video" => 0.55,
            "form" => 0.5,
            "img" => 0.4,
            "text" | "paragraph" => 0.3,
            _ => 0.2,
        }
    }

    /// Ska denna roll inkluderas för ett visst mål-kategori?
    /// Containers (nav, form, list) inkluderas alltid (kan ha relevanta barn).
    /// Headings inkluderas alltid (strukturellt viktiga).
    pub fn matches_goal_category(role: &str, goal_cat: GoalCategory) -> bool {
        // Containers och headings — alltid med (kan ha relevanta barn)
        if matches!(
            role,
            "navigation"
                | "complementary"
                | "main"
                | "form"
                | "list"
                | "listitem"
                | "group"
                | "table"
                | "dialog"
                | "heading"
        ) {
            return true;
        }

        match goal_cat {
            GoalCategory::Click => {
                matches!(
                    role,
                    "button"
                        | "cta"
                        | "link"
                        | "tab"
                        | "menu"
                        | "price"
                        | "product_card"
                        | "textbox"
                        | "searchbox"
                        | "select"
                        | "combobox"
                )
            }
            GoalCategory::Extract => {
                matches!(
                    role,
                    "text"
                        | "paragraph"
                        | "price"
                        | "product_card"
                        | "link"
                        | "data"
                        | "img"
                        | "heading"
                        | "button"
                        | "cta"
                        | "table"
                        | "row"
                        | "cell"
                        | "listitem"
                        | "generic"
                )
            }
            GoalCategory::Form => matches!(
                role,
                "textbox"
                    | "searchbox"
                    | "checkbox"
                    | "radio"
                    | "switch"
                    | "select"
                    | "combobox"
                    | "button"
                    | "cta"
            ),
            GoalCategory::Navigate => matches!(role, "link" | "button" | "cta" | "tab"),
            GoalCategory::Generic => true,
        }
    }
}

// ─── Goal Category ──────────────────────────────────────────────────────────

/// Mål-kategori för smart DOM-filtrering.
/// Klassificeras en gång vid start, styr vilka noder som traverseras.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum GoalCategory {
    /// "click buy button", "add to cart", "press submit"
    Click,
    /// "find price", "show latest news", "list products", "read article"
    Extract,
    /// "fill login form", "enter email", "sign up"
    Form,
    /// "go to next page", "navigate home", "click back"
    Navigate,
    /// Oklart mål — inkludera allt
    Generic,
}

impl GoalCategory {
    /// Klassificera en goal-sträng till kategori (O(1) keyword-lookup).
    /// Returnerar Generic om inget mönster matchar → ingen filtrering.
    pub fn from_goal(goal: &str) -> Self {
        let g = goal.to_lowercase();

        // Form: fylla i, logga in, registrera
        if g.contains("fill")
            || g.contains("enter")
            || g.contains("login")
            || g.contains("log in")
            || g.contains("sign in")
            || g.contains("sign up")
            || g.contains("register")
            || g.contains("submit form")
            || g.contains("logga in")
            || g.contains("fyll i")
            || g.contains("registrera")
        {
            return GoalCategory::Form;
        }

        // Click: köp, tryck, klicka, add to cart
        if g.contains("click")
            || g.contains("press")
            || g.contains("tap")
            || g.contains("buy")
            || g.contains("purchase")
            || g.contains("add to cart")
            || g.contains("checkout")
            || g.contains("klicka")
            || g.contains("köp")
            || g.contains("lägg i varukorg")
        {
            return GoalCategory::Click;
        }

        // Navigate: gå till, nästa sida, tillbaka
        if g.contains("next page")
            || g.contains("previous")
            || g.contains("go to")
            || g.contains("navigate")
            || g.contains("back")
            || g.contains("nästa")
            || g.contains("tillbaka")
            || g.contains("gå till")
        {
            return GoalCategory::Navigate;
        }

        // Extract: hitta, visa, sök, läs, hämta
        if g.contains("find")
            || g.contains("show")
            || g.contains("search")
            || g.contains("list")
            || g.contains("read")
            || g.contains("get")
            || g.contains("extract")
            || g.contains("price")
            || g.contains("article")
            || g.contains("news")
            || g.contains("product")
            || g.contains("download")
            || g.contains("install")
            || g.contains("hitta")
            || g.contains("visa")
            || g.contains("sök")
            || g.contains("läs")
            || g.contains("hämta")
        {
            return GoalCategory::Extract;
        }

        GoalCategory::Generic
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
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 AetherAgent/0.2".to_string()
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
            cookies: String::new(),
        }
    }
}
