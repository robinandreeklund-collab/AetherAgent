// Konsoliderade verktyg – 12 unified tools (Fas 19 prep)
//
// Varje tool-modul exponerar en huvudfunktion som tar emot ett Request-objekt
// och returnerar ett JSON-serialiserat svar. Auto-detect av input,
// automatisk säkerhet (firewall + injection-scan), och streaming-stöd.

pub mod act_tool;
pub mod collab_tool;
pub mod diff_tool;
pub mod discover_tool;
pub mod parse_hybrid_tool;
pub mod parse_tool;
pub mod plan_tool;
pub mod search_tool;
pub mod secure_tool;
pub mod session_tool;
pub mod stream_tool;
pub mod vision_tool;
pub mod workflow_tool;

use serde::{Deserialize, Serialize};

// ─── Gemensamma typer ────────────────────────────────────────────────────────

/// Gemensamt resultat-omslag för alla verktyg
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Lyckat resultat-data (JSON value)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Felmeddelande om något gick fel
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Injection-varningar funna under bearbetning
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub injection_warnings: Vec<crate::types::InjectionWarning>,
    /// Firewall-varning om URL blockerades
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firewall_blocked: Option<String>,
    /// Total tid i ms
    pub time_ms: u64,
}

impl ToolResult {
    pub fn ok(data: serde_json::Value, time_ms: u64) -> Self {
        Self {
            data: Some(data),
            error: None,
            injection_warnings: vec![],
            firewall_blocked: None,
            time_ms,
        }
    }

    pub fn err(msg: impl Into<String>, time_ms: u64) -> Self {
        Self {
            data: None,
            error: Some(msg.into()),
            injection_warnings: vec![],
            firewall_blocked: None,
            time_ms,
        }
    }

    pub fn with_warnings(mut self, warnings: Vec<crate::types::InjectionWarning>) -> Self {
        self.injection_warnings = warnings;
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!(r#"{{"error":"Serialization failed: {e}"}}"#))
    }
}

// ─── Input auto-detect ──────────────────────────────────────────────────────

/// Typ av input som detekterats
#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    /// Rå HTML-sträng
    Html(String),
    /// URL att fetcha
    Url(String),
    /// Base64-kodad PNG-screenshot
    Screenshot(String),
}

/// Detektera input-typ från de tre möjliga fälten.
/// Prioritet: screenshot > url > html (screenshot är mest specifik).
pub fn detect_input(
    html: Option<&str>,
    url: Option<&str>,
    screenshot_b64: Option<&str>,
) -> Result<InputKind, String> {
    if let Some(s) = screenshot_b64 {
        if !s.is_empty() {
            return Ok(InputKind::Screenshot(s.to_string()));
        }
    }
    if let Some(u) = url {
        if !u.is_empty() {
            return Ok(InputKind::Url(u.to_string()));
        }
    }
    if let Some(h) = html {
        if !h.is_empty() {
            return Ok(InputKind::Html(h.to_string()));
        }
    }
    Err("Ingen input angiven: ange url, html, eller screenshot_b64".to_string())
}

// ─── Säkerhetshjälpare ──────────────────────────────────────────────────────

/// Kör firewall-klassificering på en URL. Returnerar None om godkänd,
/// Some(reason) om blockerad.
pub fn firewall_check(url: &str, goal: &str) -> Option<String> {
    let config = crate::firewall::FirewallConfig::default();
    let verdict = crate::firewall::classify_request(url, goal, &config);
    if verdict.allowed {
        None
    } else {
        Some(verdict.reason)
    }
}

/// Scanna text för prompt injection. Returnerar varningar (kan vara tom).
pub fn injection_scan(text: &str) -> Vec<crate::types::InjectionWarning> {
    let mut warnings = vec![];
    let (_, warning) = crate::trust::analyze_text(0, text);
    if let Some(w) = warning {
        warnings.push(w);
    }
    warnings
}

/// Tidsstämpel i millisekunder
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ─── Intern parse-hjälpare ──────────────────────────────────────────────────

/// Bygg semantiskt träd från HTML (delegerar till lib.rs build_tree-logiken)
pub fn build_tree(html: &str, goal: &str, url: &str) -> crate::types::SemanticTree {
    let mut arena = crate::arena_dom_sink::parse_html_to_arena(html);
    arena.resolve_lazy_images();
    let title = arena.extract_title();
    let mut builder = crate::semantic::SemanticBuilder::new(goal);
    let mut tree = builder.build_from_arena(&arena, url, &title);

    // Tier 0: Hydration extraction
    if let Some(hydration_data) = crate::hydration::extract_hydration_state(html) {
        let hydration_result = crate::hydration::hydration_to_nodes(&hydration_data, goal);
        let existing_count = tree.nodes.len() as u32;
        for mut node in hydration_result.nodes {
            node.id += existing_count;
            tree.nodes.push(node);
        }
        tree.injection_warnings.extend(hydration_result.warnings);
    }

    tree.parse_time_ms = 0;
    tree
}

/// Kör lifecycle-parse med JS-eval om tillgängligt.
///
/// Strategi (FIX-SPA-001):
/// 1. Om JS muterade DOM:en → bygg träd från modifierad arena-HTML
/// 2. Om JS hade fel → logga, fall tillbaka på pre-JS DOM (original HTML)
/// 3. Om JS körde utan mutationer och utan fel → prova arena-HTML
///    Pre-JS DOM bevaras ALLTID som fallback vid JS-fel.
#[allow(unused_variables)]
pub fn build_tree_with_js(html: &str, goal: &str, url: &str) -> crate::types::SemanticTree {
    #[cfg(all(feature = "js-eval", feature = "blitz"))]
    {
        let scripts = crate::js_eval::extract_ordered_scripts(html);
        if !scripts.is_empty() {
            let rcdom = crate::parser::parse_html(html);
            let arena = crate::arena_dom::ArenaDom::from_rcdom(&rcdom);
            let eval_result = crate::dom_bridge::eval_js_with_lifecycle_and_arena(&scripts, arena);

            // Samla runtime fetch-URLs (BUGG J)
            let runtime_urls = eval_result.result.fetched_urls.clone();

            // Logga JS-fel om det uppstod
            if let Some(ref err) = eval_result.result.error {
                eprintln!("[JS] Eval error (falling back to pre-JS DOM): {err}");
            }

            // Om JS muterade DOM:en — använd den modifierade arenan
            if !eval_result.result.mutations.is_empty() {
                let modified_html = eval_result.arena.serialize_html(eval_result.arena.document);
                let mut tree = build_tree(&modified_html, goal, url);
                collect_pending_urls(&mut tree, html, &runtime_urls);
                return tree;
            }

            // Om JS körde utan fel men utan mutationer
            if eval_result.result.error.is_none() {
                let arena_html = eval_result.arena.serialize_html(eval_result.arena.document);
                if arena_html.len() > 10 && arena_html != html {
                    let mut tree = build_tree(&arena_html, goal, url);
                    collect_pending_urls(&mut tree, html, &runtime_urls);
                    return tree;
                }
            }

            // Fallback: pre-JS DOM — bifoga alla fetch-URLs för async
            let mut tree = build_tree(html, goal, url);
            collect_pending_urls(&mut tree, html, &runtime_urls);
            return tree;
        }
    }
    let mut tree = build_tree(html, goal, url);
    collect_pending_urls(&mut tree, html, &[]);
    tree
}

/// Samla pending fetch-URLs från statisk extraktion + runtime JS-capture.
fn collect_pending_urls(
    tree: &mut crate::types::SemanticTree,
    html: &str,
    runtime_urls: &[String],
) {
    let mut urls: Vec<String> = Vec::new();

    // Statisk: hitta fetch()/XHR i inline scripts
    let captures = crate::js_bridge::extract_xhr_from_snippets(html);
    for c in &captures {
        if !c.url.is_empty() && c.url.starts_with("http") {
            urls.push(c.url.clone());
        }
    }

    // Runtime: URLs som JS anropade via fetch()
    for u in runtime_urls {
        if !u.is_empty() && u.starts_with("http") && !urls.contains(u) {
            urls.push(u.clone());
        }
    }

    urls.sort();
    urls.dedup();
    tree.pending_fetch_urls = urls;
}

/// Resolve pending fetch-URLs: hämta via intercept_xhr och merge-a noder i trädet.
///
/// Anropas i async-kontext (server/MCP) efter att trädet är byggt.
/// Använder befintlig `intercept_xhr()` infrastruktur (firewall + reqwest + node-skapning).
#[cfg(feature = "fetch")]
pub async fn resolve_pending_fetches(tree: &mut crate::types::SemanticTree, goal: &str) {
    if tree.pending_fetch_urls.is_empty() {
        return;
    }

    let captures: Vec<crate::intercept::XhrCapture> = tree
        .pending_fetch_urls
        .iter()
        .map(|url| crate::intercept::XhrCapture {
            url: url.clone(),
            method: "GET".to_string(),
            headers: std::collections::HashMap::new(),
        })
        .collect();

    let config = crate::intercept::InterceptConfig {
        enabled: true,
        max_requests: 20,
        timeout_ms: 5000,
    };
    let fw_config = crate::firewall::FirewallConfig::default();

    let result = crate::intercept::intercept_xhr(&captures, goal, &config, &fw_config).await;

    // Merge nodes med unika IDs
    let existing_max_id = count_max_node_id(&tree.nodes);
    for (i, mut node) in result.nodes.into_iter().enumerate() {
        node.id = existing_max_id + 1 + i as u32;
        tree.nodes.push(node);
    }

    tree.xhr_intercepted += result.intercepted_count;
    tree.xhr_blocked += result.blocked_count;
    tree.injection_warnings
        .extend(result.xhr_injection_warnings);
    tree.pending_fetch_urls.clear();
}

/// Hitta högsta nod-ID i trädet (rekursivt)
#[cfg(feature = "fetch")]
fn count_max_node_id(nodes: &[crate::types::SemanticNode]) -> u32 {
    let mut max_id = 0u32;
    for node in nodes {
        max_id = max_id.max(node.id);
        max_id = max_id.max(count_max_node_id(&node.children));
    }
    max_id
}

/// Räkna alla noder rekursivt (inkl. barn)
pub fn count_all_nodes(nodes: &[crate::types::SemanticNode]) -> usize {
    let mut count = nodes.len();
    for node in nodes {
        count += count_all_nodes(&node.children);
    }
    count
}

/// Sortera noder efter relevans (högst först) — sorterar rotnoder
pub fn sort_by_relevance(tree: &mut crate::types::SemanticTree) {
    tree.nodes
        .sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
}

/// Flattena alla noder rekursivt till en platt lista
fn flatten_all_nodes(nodes: &[crate::types::SemanticNode]) -> Vec<crate::types::SemanticNode> {
    let mut result = Vec::new();
    for node in nodes {
        result.push(node.clone_shallow());
        result.extend(flatten_all_nodes(&node.children));
    }
    result
}

/// Begränsa till top N mest relevanta noder.
///
/// Flattenar hela trädet, sorterar efter relevans, tar top N.
/// Returnerar platt lista (inga children) — rätt beteende för top_n
/// där agenten vill ha de N bästa noderna oavsett trädposition.
pub fn limit_top_n(tree: &mut crate::types::SemanticTree, top_n: u32) {
    if top_n == 0 {
        return;
    }
    // Flattena hela trädet
    let mut flat = flatten_all_nodes(&tree.nodes);
    // Sortera efter relevans (högst först)
    flat.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
    // Ta top N
    flat.truncate(top_n as usize);
    tree.nodes = flat;
}

/// Konvertera semantiskt träd till markdown
pub fn tree_to_markdown(tree: &crate::types::SemanticTree) -> String {
    let mut md = String::with_capacity(tree.nodes.len() * 80);
    if !tree.title.is_empty() {
        md.push_str("# ");
        md.push_str(&tree.title);
        md.push('\n');
    }
    if !tree.url.is_empty() {
        md.push_str("> ");
        md.push_str(&tree.url);
        md.push_str("\n\n");
    }
    for node in &tree.nodes {
        node_to_markdown(node, &mut md, 0);
    }
    md
}

fn node_to_markdown(node: &crate::types::SemanticNode, md: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);

    match node.role.as_str() {
        "heading" => {
            let level = (depth + 1).min(6);
            md.push_str(&"#".repeat(level));
            md.push(' ');
            md.push_str(&node.label);
            md.push('\n');
        }
        "link" => {
            md.push_str(&indent);
            md.push_str("- [");
            md.push_str(&node.label);
            md.push_str("](");
            md.push_str(node.action.as_deref().unwrap_or("#"));
            md.push_str(")\n");
        }
        "button" => {
            md.push_str(&indent);
            md.push_str("- **[");
            md.push_str(&node.label);
            md.push_str("]**\n");
        }
        "textbox" | "searchbox" | "combobox" | "spinbutton" => {
            md.push_str(&indent);
            md.push_str("- `");
            md.push_str(&node.label);
            if let Some(ref v) = node.value {
                md.push_str("` = \"");
                md.push_str(v);
                md.push('"');
            } else {
                md.push('`');
            }
            md.push('\n');
        }
        "img" | "image" => {
            md.push_str(&indent);
            md.push_str("- ![");
            md.push_str(&node.label);
            md.push_str("](");
            md.push_str(node.action.as_deref().unwrap_or(""));
            md.push_str(")\n");
        }
        "text" => {
            if !node.label.is_empty() {
                md.push_str(&indent);
                md.push_str(&node.label);
                md.push('\n');
            }
        }
        _ => {
            if !node.label.is_empty() {
                md.push_str(&indent);
                md.push_str("- ");
                md.push_str(&node.label);
                md.push('\n');
            }
        }
    }

    for child in &node.children {
        node_to_markdown(child, md, depth + 1);
    }
}

/// Serialisera till JSON med pre-allokerad buffert
pub fn serialize_json<T: serde::Serialize>(
    value: &T,
    estimated_nodes: usize,
) -> Result<String, String> {
    let capacity = (estimated_nodes * 200).max(1024);
    let mut buf = Vec::with_capacity(capacity);
    serde_json::to_writer(&mut buf, value).map_err(|e| format!("Serialization failed: {e}"))?;
    // SAFETY: serde_json::to_writer producerar alltid giltig UTF-8
    Ok(unsafe { String::from_utf8_unchecked(buf) })
}
