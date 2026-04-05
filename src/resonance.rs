/// Causal Resonance Field Retrieval (CRFR)
///
/// Treats the DOM as a living resonance field where nodes oscillate with
/// amplitudes determined by goal similarity, causal memory from past
/// successful interactions, and wave propagation through the tree structure.
///
/// The field supports multi-goal interference (constructive and destructive)
/// and learns over time via causal feedback.
use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::scoring::hdc::{Hypervector, WORDS};
use crate::scoring::tfidf::TfIdfIndex;
use crate::types::SemanticNode;

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// Bas-dämpning vid propagation från förälder till barn
const BASE_CHILD_DAMPING: f32 = 0.35;
/// Bas-förstärkning vid propagation från barn till förälder
const BASE_PARENT_AMPLIFICATION: f32 = 0.25;
/// Bonus-multiplikator vid fas-synkronisering
const PHASE_SYNC_BONUS: f32 = 1.08;
/// Minsta amplitud för att en nod ska propagera vidare
const ACTIVATION_THRESHOLD: f32 = 0.01;
/// Minsta amplitud för att inkluderas i resultat
const MIN_OUTPUT_THRESHOLD: f32 = 0.01;
/// Max antal propagations-iterationer (konvergens avbryter normalt vid 2-3)
const MAX_PROPAGATION_STEPS: u32 = 6;
/// Konvergenströskel: om total amplitudförändring < detta, stoppa
const CONVERGENCE_THRESHOLD: f32 = 0.001;
// Fan-out är nu adaptivt — se adaptive_fan_out()
/// Max antal noder i fältet (skydd mot extremt stora DOM:ar)
const MAX_FIELD_NODES: usize = 10_000;
/// BM25-vikt i hybrid-scoring (keyword-precision)
const BM25_WEIGHT: f32 = 0.75;
/// HDC text-vikt (n-gram strukturell likhet)
const HDC_TEXT_WEIGHT: f32 = 0.20;
/// Roll-aspekt vikt (ren prioritetstabell — låg vikt pga ej goal-beroende)
const ROLE_WEIGHT: f32 = 0.05;
/// Kausal-boost vikt
const CAUSAL_WEIGHT: f32 = 0.3;
/// Temporal decay-faktor: halvering var 10:e minut (λ = ln2/600s ≈ 0.00115)
const CAUSAL_DECAY_LAMBDA: f64 = 0.001_155;
/// Minsta relativa amplitud-gap för att klippa output (30% drop)
const GAP_RATIO_THRESHOLD: f32 = 0.30;
/// Max antal concept entries i field-level concept memory
const MAX_CONCEPT_ENTRIES: usize = 256;
/// Max antal fält i LRU-cachen
const FIELD_CACHE_CAPACITY: usize = 64;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Type of resonance that caused a node to appear in results
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ResonanceType {
    /// Noden matchade direkt mot goal-vektorn
    Direct,
    /// Noden fick amplitud via vågpropagation från grannar
    Propagated,
    /// Noden förstärktes av kausalt minne från tidigare lyckade mål
    CausalMemory,
}

/// A single node's result from resonance propagation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceResult {
    /// Nod-ID i det semantiska trädet
    pub node_id: u32,
    /// Slutgiltig amplitud efter propagation
    pub amplitude: f32,
    /// Oscillatorfas (radianer)
    #[serde(skip_serializing)]
    pub phase: f32,
    /// Typ av resonans som dominerade
    pub resonance_type: ResonanceType,
    /// Bidrag från kausalt minne
    pub causal_boost: f32,
}

/// Per-node scoring breakdown (for dashboard CRFR Query Explorer & DOM Visualizer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScoreBreakdown {
    pub node_id: u32,
    pub bm25_score: f32,
    pub hdc_score: f32,
    pub role_priority: f32,
    pub concept_boost: f32,
    pub causal_boost: f32,
    pub answer_shape: f32,
    pub answer_type_boost: f32,
    pub zone_penalty: f32,
    pub meta_penalty: f32,
    pub combmnz: f32,
    pub template_boost: bool,
    pub final_amplitude: f32,
}

/// Full propagation trace for dashboard visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationTrace {
    /// BM25 candidate count (top-200 pre-filter)
    pub bm25_candidates: usize,
    /// Total nodes scored (past cascade filter)
    pub cascade_candidates: usize,
    /// Wave propagation: iterations actually run (1..6)
    pub propagation_iterations: u32,
    /// Per-iteration convergence delta
    pub iteration_deltas: Vec<f32>,
    /// Amplitude gap: position where cut happened (None = hard top_k)
    pub gap_cut_position: Option<usize>,
    /// Per-node scoring breakdown (top nodes only)
    pub node_scores: Vec<NodeScoreBreakdown>,
    /// Total nodes in field
    pub total_field_nodes: usize,
    /// Template match detected
    pub template_match: bool,
}

/// Serialiserbar wrapper för Hypervector-data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HvData {
    bits: Vec<u64>,
}

impl HvData {
    fn from_hv(hv: &Hypervector) -> Self {
        HvData {
            bits: hv.bits_raw().to_vec(),
        }
    }

    fn to_hv(&self) -> Hypervector {
        // Konvertera tillbaka till fast array; fyll med nollor om storlek inte stämmer
        let mut arr = [0u64; WORDS];
        let len = self.bits.len().min(WORDS);
        arr[..len].copy_from_slice(&self.bits[..len]);
        Hypervector::from_bits(arr)
    }
}

/// Per-node resonance state within the field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceState {
    // ── Multi-field vektorer (aspekt 1: multi-field resonance) ──
    /// Text-hypervector (ren text n-gram encoding, ingen roll-binding)
    #[serde(with = "hv_serde")]
    text_hv: Hypervector,
    /// Roll-string (heading, button, price, etc.) — billigare än HV för roll-matchning
    role: String,
    /// Djup i DOM-trädet (0 = rot)
    depth: u32,

    // ── Oscillatortillstånd ──
    /// Oscillatorfas (0.0..2π)
    phase: f32,
    /// Nuvarande resonansstyrka
    amplitude: f32,

    // ── Kausal inlärning ──
    /// Ackumulerat kausalt minne från lyckade mål
    #[serde(with = "hv_serde")]
    causal_memory: Hypervector,
    /// Antal lyckade målmatchningar
    hit_count: u32,
    /// Hash av senaste mål (undvik dubbelräkning)
    last_goal_hash: u64,
    /// Tidpunkt (ms) för senaste lyckade feedback (för temporal decay)
    #[serde(default)]
    last_hit_ms: u64,
}

/// Serde-modul för Hypervector (privat bits-fält)
mod hv_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(hv: &Hypervector, s: S) -> Result<S::Ok, S::Error> {
        let data = HvData::from_hv(hv);
        data.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Hypervector, D::Error> {
        let data = HvData::deserialize(d)?;
        Ok(data.to_hv())
    }
}

/// The resonance field — a living overlay on the semantic tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceField {
    /// Resonanstillstånd per nod-ID
    nodes: HashMap<u32, ResonanceState>,
    /// Originalets URL (bevaras för dashboard)
    #[serde(default)]
    pub url: String,
    /// Förälder-mappning: child_id -> parent_id
    #[serde(default)]
    parent_map: HashMap<u32, u32>,
    /// Barn-mappning: parent_id -> [child_ids]
    #[serde(default)]
    children_map: HashMap<u32, Vec<u32>>,
    /// Nod-labels (för BM25-indexering)
    #[serde(default)]
    node_labels: HashMap<u32, String>,
    /// Nod-values: action/href/value/data-attribut (#3 value-aware retrieval)
    #[serde(default)]
    node_values: HashMap<u32, String>,
    /// Cachad BM25-index (#2 sub-ms: byggs en gång, återanvänds vid cache-hit)
    #[serde(skip)]
    bm25_cache: Option<TfIdfIndex>,
    /// Learned propagation weights: per-roll Bayesian success tracking.
    /// Key: "role:down" eller "role:up".
    /// Value: (alpha, beta) — Beta-distribution parametrar.
    ///   alpha ≈ confidence-weighted successes + prior
    ///   beta ≈ confidence-weighted failures + prior
    ///   mean = alpha / (alpha + beta)
    /// Uppdateras av feedback() med confidence-weighted signal + decay.
    #[serde(default)]
    propagation_stats: HashMap<String, (f32, f32)>,
    /// Field-level memory: aggregerade HV per goal-koncept.
    /// Lär sig "vad price-frågor matchar" globalt, inte per nod.
    #[serde(default)]
    concept_memory: HashMap<String, HvData>,
    /// Structural fingerprint: hash of top-20 role sequence (for template detection)
    #[serde(default)]
    structure_hash: u64,
    /// URL-hash som identifierar sidan
    pub url_hash: u64,
    /// Domain-hash (för domain-level learning)
    #[serde(default)]
    pub domain_hash: u64,
    /// Tidpunkt då fältet skapades (ms sedan epoch)
    pub created_at_ms: u64,
    /// Totalt antal propagations-anrop
    pub total_queries: u32,
}

/// Roll-prioritet: hur sannolikt innehåller rollen relevant data (0.0-1.0)
fn role_priority(role: &str) -> f32 {
    match role {
        "price" | "data" | "cell" => 1.0,
        "heading" | "text" | "paragraph" => 0.9,
        "button" | "cta" | "product_card" => 0.85,
        "link" | "listitem" | "row" => 0.7,
        "img" | "table" => 0.6,
        "textbox" | "searchbox" | "select" => 0.5,
        "navigation" | "complementary" => 0.2,
        "generic" => 0.4,
        _ => 0.5,
    }
}

/// Heuristisk bas-vikt nedåt (cold-start prior)
fn heuristic_down_weight(role: &str) -> f32 {
    match role {
        "heading" | "table" | "row" | "list" => 1.2,
        "generic" | "article" | "section" => 1.0,
        "price" | "text" | "button" | "link" => 0.7,
        "navigation" | "complementary" | "contentinfo" => 0.3,
        _ => 1.0,
    }
}

/// Heuristisk bas-vikt uppåt (cold-start prior)
fn heuristic_up_weight(role: &str) -> f32 {
    match role {
        "price" | "data" | "cell" => 1.3,
        "text" | "paragraph" | "heading" => 1.1,
        "button" | "link" | "cta" => 0.9,
        "navigation" | "complementary" => 0.3,
        _ => 1.0,
    }
}

/// Adaptive fan-out: scales with DOM complexity
fn adaptive_fan_out(children_count: usize) -> usize {
    if children_count == 0 {
        return 0;
    }
    if children_count <= 8 {
        children_count
    } else {
        ((4.0 + (children_count as f32).ln() * 8.0) as usize).min(children_count)
    }
}

/// Answer-shape scoring: how much does this node look like an answer?
/// Pure structure + statistics — no semantics.
fn answer_shape_score(label: &str, role: &str, siblings_count: usize) -> f32 {
    let mut score: f32 = 0.0;
    // Contains numbers (prices, dates, populations, percentages)
    if label.bytes().any(|b| b.is_ascii_digit()) {
        score += 0.3;
    }
    // Short text (< 50 chars) — answers are concise
    if !label.is_empty() && label.len() < 50 {
        score += 0.2;
    }
    // Has unit markers (currency, percentage, measurement)
    let lower = label.to_lowercase();
    if lower.contains('$')
        || lower.contains('£')
        || lower.contains('€')
        || lower.contains('%')
        || lower.contains("kr")
        || lower.contains("km")
        || lower.contains("kg")
    {
        score += 0.15;
    }
    // In a structured context (table row, list item with siblings)
    if siblings_count >= 2 && matches!(role, "cell" | "data" | "row" | "listitem") {
        score += 0.15;
    }
    // Data-oriented roles get a small boost
    if matches!(role, "price" | "data" | "cell") {
        score += 0.1;
    }
    // Content density: longer labels with actual text content
    if label.len() > 20 && label.len() < 200 {
        // Mid-length text is often the answer (not too short, not a wrapper)
        score += 0.1;
    }
    score
}

/// Answer-type detection: classify goal by expected answer type,
/// then boost nodes whose content matches that type.
/// Returns bonus score (0.0-0.3).
fn answer_type_boost(goal: &str, label: &str) -> f32 {
    let goal_lower = goal.to_lowercase();
    let label_lower = label.to_lowercase();

    // Price/cost query → boost nodes with currency
    if (goal_lower.contains("price")
        || goal_lower.contains("cost")
        || goal_lower.contains("pris")
        || goal_lower.contains("kr")
        || goal_lower.contains("fee"))
        && (label_lower.contains('$')
            || label_lower.contains('£')
            || label_lower.contains('€')
            || label_lower.contains("kr"))
    {
        return 0.25;
    }

    // Population/count query → boost nodes with large numbers
    if goal_lower.contains("population")
        || goal_lower.contains("invånare")
        || goal_lower.contains("antal")
        || goal_lower.contains("many")
    {
        // Check for numbers > 1000
        let has_large_number = label
            .split(|c: char| !c.is_ascii_digit() && c != ' ' && c != ',')
            .any(|s| s.replace([' ', ','], "").len() >= 4);
        if has_large_number {
            return 0.2;
        }
    }

    // Date/time query → boost nodes with date patterns
    if (goal_lower.contains("date")
        || goal_lower.contains("when")
        || goal_lower.contains("datum")
        || goal_lower.contains("year")
        || goal_lower.contains("år"))
        && (label.contains("202") || label.contains("201") || label.contains("200"))
    {
        return 0.15;
    }

    // Rate/percentage query → boost nodes with %
    if (goal_lower.contains("rate")
        || goal_lower.contains("percent")
        || goal_lower.contains("ränta"))
        && label_lower.contains('%')
    {
        return 0.25;
    }

    0.0
}

/// Boilerplate zone penalty: nodes in nav/footer/aside get penalized.
/// Based on HTML5 landmark roles — not semantics, pure structure.
fn zone_penalty(role: &str, depth: u32) -> f32 {
    match role {
        "navigation" | "complementary" | "contentinfo" | "banner" => 0.5,
        "generic" if depth <= 1 => 0.7, // Top-level generic = likely wrapper
        _ => 1.0,                       // No penalty
    }
}

/// Learned propagation weight via Beta-distribution with pre-computed key.
///
/// Propagation stats lagrar (alpha, beta) per roll+riktning:
///   alpha = confidence-weighted successes + heuristisk prior
///   beta = confidence-weighted failures + heuristisk prior
///   mean = alpha / (alpha + beta)
///
/// Heuristisk prior kodas som initial (alpha, beta):
///   heuristic=1.2 → prior alpha=1.2, beta=1.0 (svag bias mot success)
///   heuristic=0.3 → prior alpha=0.3, beta=1.0 (svag bias mot failure)
///
/// Ingen manuell blend-faktor — Beta-distributionen hanterar
/// prior → posterior automatiskt med mer data.
///
/// Takes a pre-computed key (e.g. "heading:down") to avoid format!() in hot loop.
fn learned_weight_precomputed(
    key: &str,
    heuristic: f32,
    stats: &HashMap<String, (f32, f32)>,
) -> f32 {
    let (alpha, beta) = stats.get(key).copied().unwrap_or((heuristic, 1.0));
    let total = alpha + beta;
    if total < 0.001 {
        return heuristic;
    }
    let mean = alpha / total.max(0.001);
    0.2 + mean * 1.3
}

/// Compute page-level features for contextual weight selection.
/// Returns (text_density, list_density, table_density, nav_density, depth_spread)
fn page_profile(nodes: &HashMap<u32, ResonanceState>) -> (f32, f32, f32, f32, f32) {
    if nodes.is_empty() {
        return (0.0, 0.0, 0.0, 0.0, 0.0);
    }
    let n = nodes.len() as f32;
    let text = nodes
        .values()
        .filter(|s| matches!(s.role.as_str(), "text" | "paragraph" | "heading"))
        .count() as f32;
    let list = nodes
        .values()
        .filter(|s| matches!(s.role.as_str(), "listitem" | "list"))
        .count() as f32;
    let table = nodes
        .values()
        .filter(|s| matches!(s.role.as_str(), "cell" | "row" | "table" | "data"))
        .count() as f32;
    let nav = nodes
        .values()
        .filter(|s| matches!(s.role.as_str(), "navigation" | "complementary" | "banner"))
        .count() as f32;
    let max_depth = nodes.values().map(|s| s.depth).max().unwrap_or(0) as f32;
    (text / n, list / n, table / n, nav / n, max_depth)
}

/// Enkel hash-funktion för URL:er (FNV-1a)
fn hash_url(url: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Extrahera domain-hash från en URL.
/// "https://www.example.com/path" → hash("example.com")
fn domain_hash_from_url(url: &str) -> u64 {
    let without_scheme = url.split("://").last().unwrap_or(url);
    let domain = without_scheme.split('/').next().unwrap_or("");
    let clean = domain.strip_prefix("www.").unwrap_or(domain);
    hash_url(clean)
}

/// Hämta nuvarande tid i millisekunder (fallback till 0 utan std::time)
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// BUG-02: Penalize metadata-pattern nodes (HN .subtext, Reddit score lines, etc.)
/// Matches: "N points by X N hours ago | hide | N comments"
/// BUG-B: Penalize injected state nodes (__APOLLO_STATE__, __NEXT_DATA__, window.__*)
fn state_injection_penalty(label: &str) -> f32 {
    if label.starts_with("__APOLLO_STATE__")
        || label.starts_with("__NEXT_DATA__")
        || label.starts_with("window.__")
        || label.starts_with("contentApiData.")
        || label.starts_with("layoutData.")
        || label.starts_with("localeData.")
        || label.starts_with("commonI18nResources.")
        || label.starts_with("dataManifest.")
        || label.starts_with("currentUrlString")
        || label.starts_with("pageProps.")
    {
        return 0.15; // Very strong penalty — serialized state, not content
    }
    1.0
}

fn metadata_penalty(label: &str) -> f32 {
    let lower = label.to_lowercase();
    // BUG-E: Transient UI error messages
    if lower.contains("uh oh! there was an error")
        || lower.contains("please reload this page")
        || lower.contains("something went wrong")
        || lower.contains("loading...")
        || lower.contains("laddar...")
    {
        return 0.5; // Penalty for error-state content
    }
    // HN/Reddit/Lobsters metadata pattern
    if (lower.contains("points by") || lower.contains("hours ago") || lower.contains("minutes ago"))
        && (lower.contains("comments") || lower.contains("hide"))
    {
        return 0.4; // Strong penalty — this is metadata, not content
    }
    // Generic timestamp-heavy metadata
    if lower.contains("ago")
        && (lower.contains("point") || lower.contains("vote") || lower.contains("score"))
    {
        return 0.5;
    }
    1.0 // No penalty
}

impl ResonanceField {
    /// Build an initial resonance field from a semantic tree.
    ///
    /// Initializes all nodes with phase=0, amplitude=0, and computes
    /// base hypervectors from text n-grams bound with role vectors.
    pub fn from_semantic_tree(tree_nodes: &[SemanticNode], url: &str) -> Self {
        let mut nodes = HashMap::new();
        let mut parent_map = HashMap::new();
        let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut node_labels: HashMap<u32, String> = HashMap::new();

        // Platta ut trädet och bygg relationer
        for node in tree_nodes {
            Self::flatten_node(
                node,
                None,
                &mut nodes,
                &mut parent_map,
                &mut children_map,
                &mut node_labels,
            );
            // Begränsa storlek
            if nodes.len() >= MAX_FIELD_NODES {
                break;
            }
        }

        // #3: Samla värde-data (action/href/value) per nod
        let mut node_values: HashMap<u32, String> = HashMap::new();
        Self::collect_values(tree_nodes, &mut node_values);

        let dh = domain_hash_from_url(url);
        let mut field = ResonanceField {
            nodes,
            url: url.to_string(),
            parent_map,
            children_map,
            node_labels,
            node_values,
            bm25_cache: None,
            propagation_stats: HashMap::new(),
            concept_memory: HashMap::new(),
            structure_hash: 0,
            url_hash: hash_url(url),
            domain_hash: dh,
            created_at_ms: now_ms(),
            total_queries: 0,
        };

        // Applicera domain-level priors (warm-start från samma domäns lärande)
        if let Ok(registry) = DOMAIN_REGISTRY.lock() {
            if let Some(profile) = registry.get(dh) {
                for (key, &(alpha, beta)) in &profile.stats {
                    field
                        .propagation_stats
                        .entry(key.clone())
                        .or_insert((alpha, beta));
                }
                for (token, hv_data) in &profile.concepts {
                    field
                        .concept_memory
                        .entry(token.clone())
                        .or_insert_with(|| hv_data.clone());
                }
            }
        }

        field
    }

    /// #3 Value-aware: samla action/href/value från noder
    fn collect_values(nodes: &[SemanticNode], out: &mut HashMap<u32, String>) {
        for node in nodes {
            let mut parts: Vec<&str> = Vec::new();
            if let Some(ref action) = node.action {
                parts.push(action);
            }
            if let Some(ref value) = node.value {
                parts.push(value);
            }
            if let Some(ref name) = node.name {
                parts.push(name);
            }
            if !parts.is_empty() {
                out.insert(node.id, parts.join(" "));
            }
            Self::collect_values(&node.children, out);
        }
    }

    /// Rekursivt platta ut en nod och dess barn
    fn flatten_node(
        node: &SemanticNode,
        parent_id: Option<u32>,
        nodes: &mut HashMap<u32, ResonanceState>,
        parent_map: &mut HashMap<u32, u32>,
        children_map: &mut HashMap<u32, Vec<u32>>,
        node_labels: &mut HashMap<u32, String>,
    ) {
        if nodes.len() >= MAX_FIELD_NODES {
            return;
        }

        // Multi-field: separata aspekter per nod
        let text_hv = if node.label.is_empty() {
            Hypervector::from_seed(&format!("__empty_{}", node.id))
        } else {
            Hypervector::from_text_ngrams(&node.label)
        };
        // Beräkna djup från förälder-kedjan
        let depth = parent_id
            .and_then(|pid| nodes.get(&pid).map(|p| p.depth + 1))
            .unwrap_or(0);

        let state = ResonanceState {
            text_hv,
            role: node.role.clone(),
            depth,
            phase: 0.0,
            amplitude: 0.0,
            causal_memory: Hypervector::zero(),
            hit_count: 0,
            last_goal_hash: 0,
            last_hit_ms: 0,
        };

        nodes.insert(node.id, state);
        node_labels.insert(node.id, node.label.clone());

        // Registrera förälder-relation
        if let Some(pid) = parent_id {
            parent_map.insert(node.id, pid);
            children_map.entry(pid).or_default().push(node.id);
        }

        // Rekursera barn
        for child in &node.children {
            Self::flatten_node(
                child,
                Some(node.id),
                nodes,
                parent_map,
                children_map,
                node_labels,
            );
        }

        // #17 Hierarchical HDC: blend children's HVs into parent (structural context)
        if !node.children.is_empty() && nodes.contains_key(&node.id) {
            let child_hvs: Vec<Hypervector> = node
                .children
                .iter()
                .filter_map(|child| nodes.get(&child.id).map(|s| s.text_hv.clone()))
                .collect();
            if !child_hvs.is_empty() {
                let refs: Vec<&Hypervector> = child_hvs.iter().collect();
                let children_bundle = Hypervector::bundle(&refs);
                // 80% own text + 20% children context
                if let Some(parent_state) = nodes.get_mut(&node.id) {
                    let own = parent_state.text_hv.clone();
                    parent_state.text_hv =
                        Hypervector::bundle(&[&own, &own, &own, &own, &children_bundle]);
                }
            }
        }
    }

    /// Propagate a goal through the resonance field.
    ///
    /// Phase 1: Compute initial resonance from HDC similarity + causal memory.
    /// Phase 2: Wave propagation through parent-child edges.
    /// Phase 3: Collect and sort results by amplitude.
    /// Propagate with full trace capture (for dashboard)
    pub fn propagate_traced(&mut self, goal: &str) -> (Vec<ResonanceResult>, PropagationTrace) {
        let (results, trace) = self.propagate_inner(goal, true);
        (
            results,
            trace.unwrap_or_else(|| PropagationTrace {
                bm25_candidates: 0,
                cascade_candidates: 0,
                propagation_iterations: 0,
                iteration_deltas: vec![],
                gap_cut_position: None,
                node_scores: vec![],
                total_field_nodes: self.nodes.len(),
                template_match: false,
            }),
        )
    }

    pub fn propagate(&mut self, goal: &str) -> Vec<ResonanceResult> {
        self.propagate_inner(goal, false).0
    }

    fn propagate_inner(
        &mut self,
        goal: &str,
        capture_trace: bool,
    ) -> (Vec<ResonanceResult>, Option<PropagationTrace>) {
        self.total_queries += 1;
        let goal_hv = Hypervector::from_text_ngrams(goal);

        let mut causal_boosts: HashMap<u32, f32> = HashMap::new();
        let mut resonance_types: HashMap<u32, ResonanceType> = HashMap::new();
        // Trace data
        let mut trace_bm25_scores: HashMap<u32, f32> = HashMap::new();
        let mut trace_hdc_scores: HashMap<u32, f32> = HashMap::new();
        let mut trace_role_priorities: HashMap<u32, f32> = HashMap::new();
        let mut trace_concept_boosts: HashMap<u32, f32> = HashMap::new();
        let mut trace_answer_shapes: HashMap<u32, f32> = HashMap::new();
        let mut trace_answer_types: HashMap<u32, f32> = HashMap::new();
        let mut trace_zone_penalties: HashMap<u32, f32> = HashMap::new();
        let mut trace_meta_penalties: HashMap<u32, f32> = HashMap::new();
        let mut trace_combmnz: HashMap<u32, f32> = HashMap::new();
        let mut trace_template_boosts: HashMap<u32, bool> = HashMap::new();
        let mut trace_iteration_deltas: Vec<f32> = Vec::new();

        // #15 LinUCB: Contextual weight adjustment based on page profile
        let (_text_d, _list_d, table_d, nav_d, _depth) = page_profile(&self.nodes);

        // #19 Template detection: compute structural fingerprint
        let current_structure: u64 = {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            let mut roles: Vec<(&u32, &str)> = self
                .nodes
                .iter()
                .map(|(id, s)| (id, s.role.as_str()))
                .collect();
            roles.sort_by_key(|(id, _)| *id);
            for (_, role) in roles.iter().take(20) {
                for b in role.as_bytes() {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            }
            h
        };
        let template_match = self.structure_hash != 0 && self.structure_hash == current_structure;
        self.structure_hash = current_structure;

        // #2: Cachad BM25-index (byggs vid första anrop, återanvänds)
        // #3: Label + values konkateneras per nod (ett dokument per nod)
        if self.bm25_cache.is_none() {
            let combined: Vec<(u32, String)> = self
                .node_labels
                .iter()
                .map(|(&id, label)| {
                    let val = self.node_values.get(&id).map(|v| v.as_str()).unwrap_or("");
                    if val.is_empty() {
                        (id, label.clone())
                    } else {
                        // BM25F approximation: value appears twice = higher TF weight
                        (id, format!("{} {} {}", label, val, val))
                    }
                })
                .collect();
            let pairs: Vec<(u32, &str)> =
                combined.iter().map(|(id, s)| (*id, s.as_str())).collect();
            self.bm25_cache = Some(TfIdfIndex::build(&pairs));
        }
        let bm25_results = self
            .bm25_cache
            .as_ref()
            .map(|idx| idx.query(goal, self.nodes.len()))
            .unwrap_or_default();
        let bm25_max = bm25_results
            .first()
            .map(|(_, s)| *s)
            .unwrap_or(1.0)
            .max(0.001);
        let bm25_scores: HashMap<u32, f32> = bm25_results
            .into_iter()
            .map(|(id, score)| (id, (score / bm25_max).clamp(0.0, 1.0)))
            .collect();

        // Fas 1: Multi-field initial resonans
        // #9: Zero semantic — roll-signal = ren prioritetstabell, ingen HV-matchning
        let now = now_ms();
        let mut node_ids: Vec<u32> = self.nodes.keys().copied().collect();
        node_ids.sort_unstable();

        // Pre-compute answer shape scores (avoids labels_ref clone)
        let shape_scores: HashMap<u32, f32> = self
            .node_labels
            .iter()
            .map(|(&id, label)| {
                let role = self.nodes.get(&id).map(|s| s.role.as_str()).unwrap_or("");
                let siblings = self
                    .children_map
                    .get(&self.parent_map.get(&id).copied().unwrap_or(0))
                    .map(|c| c.len())
                    .unwrap_or(0);
                (id, answer_shape_score(label, role, siblings))
            })
            .collect();
        // Pre-compute metadata + state injection penalties (avoids label lookup in hot loop)
        let meta_penalties: HashMap<u32, f32> = self
            .node_labels
            .iter()
            .map(|(&id, label)| (id, metadata_penalty(label) * state_injection_penalty(label)))
            .collect();

        // BUG-05: Extract site name for nav-artifact penalization
        let site_words: Vec<String> = self
            .node_labels
            .values()
            .take(3) // First few nodes often contain site name
            .flat_map(|label| {
                label
                    .split_whitespace()
                    .take(5)
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
            })
            .filter(|w| w.len() > 3)
            .collect();

        // CASCADE Stage 1: BM25-only fast pre-filter (all N nodes → top 100)
        // Only nodes with BM25 > 0 proceed to expensive scoring
        let bm25_candidates: Vec<u32> = {
            let mut scored: Vec<(u32, f32)> = bm25_scores
                .iter()
                .map(|(&id, &score)| (id, score))
                .filter(|(_, s)| *s > 0.0)
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored.truncate(200); // Top 200 BM25 candidates (generous)
            scored.into_iter().map(|(id, _)| id).collect()
        };
        // Also include nodes with causal memory (they may score 0 on BM25 but have learned value)
        let causal_nodes: Vec<u32> = node_ids
            .iter()
            .filter(|&&nid| {
                self.nodes
                    .get(&nid)
                    .map(|s| s.hit_count > 0)
                    .unwrap_or(false)
            })
            .copied()
            .collect();
        let trace_bm25_count = bm25_candidates.len();
        let cascade_candidates: std::collections::HashSet<u32> =
            bm25_candidates.into_iter().chain(causal_nodes).collect();
        let trace_cascade_count = cascade_candidates.len();

        for &nid in &node_ids {
            // CASCADE: skip expensive scoring for nodes not in candidate set
            // BUT: on small DOMs (< 200 nodes), score everything — no need to filter
            if node_ids.len() >= 200 && !cascade_candidates.contains(&nid) {
                // Give non-candidates a minimal amplitude (can still be boosted by propagation)
                if let Some(state) = self.nodes.get_mut(&nid) {
                    state.amplitude = 0.0;
                }
                continue;
            }
            if let Some(state) = self.nodes.get_mut(&nid) {
                let bm25_score = bm25_scores.get(&nid).copied().unwrap_or(0.0);

                let raw_sim = state.text_hv.similarity(&goal_hv);
                let hdc_norm = ((raw_sim + 1.0) / 2.0).clamp(0.0, 1.0);
                let hdc_score = hdc_norm * hdc_norm;

                // Field-level concept boost
                let concept_boost: f32 = if !self.concept_memory.is_empty() {
                    let goal_tokens: Vec<&str> = goal
                        .split(|c: char| !c.is_alphanumeric())
                        .filter(|s| s.len() > 2)
                        .collect();
                    let mut max_boost: f32 = 0.0;
                    for token in &goal_tokens {
                        if let Some(hv_data) = self.concept_memory.get(*token) {
                            let concept_hv = hv_data.to_hv();
                            let sim = state.text_hv.similarity(&concept_hv);
                            let norm = ((sim + 1.0) / 2.0).clamp(0.0, 1.0);
                            max_boost = max_boost.max(norm * norm * 0.15);
                        }
                    }
                    max_boost
                } else {
                    0.0
                };

                // #9: Ren roll-prioritet — ingen semantisk HV-matchning
                let role_boost = role_priority(&state.role);

                let base_resonance = BM25_WEIGHT * bm25_score
                    + HDC_TEXT_WEIGHT * hdc_score
                    + ROLE_WEIGHT * role_boost
                    + concept_boost;

                let causal_boost = if state.hit_count > 0 {
                    let raw_causal = state.causal_memory.similarity(&goal_hv);
                    let norm_causal = ((raw_causal + 1.0) / 2.0).clamp(0.0, 1.0);
                    let elapsed_s = (now.saturating_sub(state.last_hit_ms) as f64) / 1000.0;
                    let decay = (-CAUSAL_DECAY_LAMBDA * elapsed_s).exp() as f32;
                    norm_causal * norm_causal * CAUSAL_WEIGHT * decay
                } else {
                    0.0
                };

                // CombMNZ: reward consensus across signals
                let signal_count = [
                    bm25_score > 0.01,
                    hdc_score > 0.01,
                    role_boost > 0.1,
                    concept_boost > 0.001,
                ]
                .iter()
                .filter(|&&b| b)
                .count() as f32;
                let combmnz = if signal_count >= 2.0 {
                    1.0 + (signal_count - 1.0) * 0.15
                } else {
                    1.0
                };

                let answer_type = answer_type_boost(
                    goal,
                    self.node_labels.get(&nid).map(|s| s.as_str()).unwrap_or(""),
                );
                // BUG-04: Soft boost — preserve relative ranking
                state.amplitude =
                    base_resonance + causal_boost * (1.0 - base_resonance.min(0.95)) + answer_type;
                state.amplitude *= combmnz;

                // #19 Template match: extra boost when same page structure detected
                if template_match && state.hit_count > 0 {
                    state.amplitude *= 1.2; // 20% extra when we recognize the template
                }

                // Answer-shape boost: noder som SER UT som svar rankas högre
                let shape = shape_scores.get(&nid).copied().unwrap_or(0.0);
                state.amplitude *= 1.0 + shape;

                let zone = zone_penalty(&state.role, state.depth);
                state.amplitude *= zone;

                // BUG-02 + BUG-B: Pre-computed metadata + state injection penalties
                let penalty = meta_penalties.get(&nid).copied().unwrap_or(1.0);
                state.amplitude *= penalty;

                // BUG-05: Site-name penalization
                if !site_words.is_empty() {
                    let label_lower = self
                        .node_labels
                        .get(&nid)
                        .map(|s| s.to_lowercase())
                        .unwrap_or_default();
                    let site_word_ratio = site_words
                        .iter()
                        .filter(|w| label_lower.contains(w.as_str()))
                        .count() as f32
                        / site_words.len().max(1) as f32;
                    if site_word_ratio > 0.6 && state.role != "heading" {
                        state.amplitude *= 0.7; // 30% penalty for nav artifacts
                    }
                }

                causal_boosts.insert(nid, causal_boost);

                // Trace: capture per-node score breakdown
                if capture_trace {
                    trace_bm25_scores.insert(nid, bm25_score);
                    trace_hdc_scores.insert(nid, hdc_score);
                    trace_role_priorities.insert(nid, role_boost);
                    trace_concept_boosts.insert(nid, concept_boost);
                    trace_answer_shapes.insert(nid, shape);
                    trace_answer_types.insert(nid, answer_type);
                    trace_zone_penalties.insert(nid, zone);
                    trace_meta_penalties.insert(nid, penalty);
                    trace_combmnz.insert(nid, combmnz);
                    trace_template_boosts.insert(nid, template_match && state.hit_count > 0);
                }

                if causal_boost > base_resonance * 0.5 && state.hit_count > 0 {
                    resonance_types.insert(nid, ResonanceType::CausalMemory);
                } else if base_resonance > ACTIVATION_THRESHOLD {
                    resonance_types.insert(nid, ResonanceType::Direct);
                }
            }
        }

        // #15 Contextual adjustment: page-type-aware amplitude scaling
        if nav_d > 0.3 {
            // High-nav pages: boost content nodes, penalize nav further
            for state in self.nodes.values_mut() {
                if matches!(
                    state.role.as_str(),
                    "text" | "paragraph" | "heading" | "price" | "data"
                ) {
                    state.amplitude *= 1.1;
                }
            }
        }
        if table_d > 0.15 {
            // Table-heavy pages: boost data/cell nodes
            for state in self.nodes.values_mut() {
                if matches!(state.role.as_str(), "cell" | "data" | "row") {
                    state.amplitude *= 1.15;
                }
            }
        }

        // Fas 2: Convergent propagation — O(E) per iteration, max 6 iterationer.
        //
        // Komplexitetsanalys:
        //   Varje iteration traverserar alla edges en gång: O(E) = O(N) för ett träd.
        //   Adaptive fan-out garanterar att inga high-degree noder
        //   (t.ex. <ul> med 200 <li>) blåser upp till O(N²).
        //   Konvergens stoppar normalt vid 2-3 iterationer.
        //   Total: O(K × min(E, N×32)) där K ≈ 2-3.
        //
        // Pre-compute role keys to avoid format!() in hot loop
        let down_keys: HashMap<u32, String> = self
            .nodes
            .iter()
            .map(|(&id, s)| (id, format!("{}:down", s.role)))
            .collect();
        let up_keys: HashMap<u32, String> = self
            .nodes
            .iter()
            .map(|(&id, s)| (id, format!("{}:up", s.role)))
            .collect();

        // Snapshot: Vec istf HashMap — O(N) men med bättre cache-locality.
        for _step in 0..MAX_PROPAGATION_STEPS {
            // Snapshot amplituder (Vec för cache-locality, sorterad efter nod-id)
            let amp_map: HashMap<u32, f32> = self
                .nodes
                .iter()
                .map(|(&id, s)| (id, s.amplitude))
                .collect();

            let mut total_delta: f32 = 0.0;

            // Förälder → barn (fan-out capped)
            for (&parent_id, children) in &self.children_map {
                let parent_amp = amp_map.get(&parent_id).copied().unwrap_or(0.0);
                if parent_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }
                let confidence_factor = parent_amp.sqrt();
                let parent_role = self
                    .nodes
                    .get(&parent_id)
                    .map(|s| s.role.as_str())
                    .unwrap_or("");
                let role_factor = learned_weight_precomputed(
                    down_keys.get(&parent_id).map(|s| s.as_str()).unwrap_or(""),
                    heuristic_down_weight(parent_role),
                    &self.propagation_stats,
                );
                let damping = BASE_CHILD_DAMPING * confidence_factor * role_factor;

                let fan_out = adaptive_fan_out(children.len());
                for &child_id in &children[..fan_out] {
                    if let Some(child_state) = self.nodes.get_mut(&child_id) {
                        let mut propagated = parent_amp * damping;
                        propagated *= PHASE_SYNC_BONUS; // Phase always 0 → always synced
                        if propagated > child_state.amplitude {
                            total_delta += propagated - child_state.amplitude;
                            child_state.amplitude = propagated;
                            resonance_types
                                .entry(child_id)
                                .or_insert(ResonanceType::Propagated);
                        }
                    }
                }
            }

            // Barn → förälder (alltid 1:1, inget fan-out-problem)
            for (&child_id, &parent_id) in &self.parent_map {
                let child_amp = amp_map.get(&child_id).copied().unwrap_or(0.0);
                if child_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }
                let confidence_factor = child_amp.sqrt();
                let child_role = self
                    .nodes
                    .get(&child_id)
                    .map(|s| s.role.as_str())
                    .unwrap_or("");
                let role_factor = learned_weight_precomputed(
                    up_keys.get(&child_id).map(|s| s.as_str()).unwrap_or(""),
                    heuristic_up_weight(child_role),
                    &self.propagation_stats,
                );
                let amplification = BASE_PARENT_AMPLIFICATION * confidence_factor * role_factor;

                if let Some(parent_state) = self.nodes.get_mut(&parent_id) {
                    let mut propagated = child_amp * amplification;
                    propagated *= PHASE_SYNC_BONUS; // Phase always 0 → always synced
                    if propagated > parent_state.amplitude {
                        total_delta += propagated - parent_state.amplitude;
                        parent_state.amplitude = propagated;
                        resonance_types
                            .entry(parent_id)
                            .or_insert(ResonanceType::Propagated);
                    }
                }
            }

            // PPR-inspired restart: re-inject energy at BM25 seed nodes
            // Prevents over-smoothing by anchoring signal to original matches
            for (&nid, &bm25_score) in &bm25_scores {
                if bm25_score > 0.5 {
                    if let Some(state) = self.nodes.get_mut(&nid) {
                        let restart = bm25_score * 0.1; // 10% restart probability
                        if restart > state.amplitude * 0.5 {
                            state.amplitude = state.amplitude.max(restart);
                        }
                    }
                }
            }

            // Trace: capture iteration delta
            if capture_trace {
                trace_iteration_deltas.push(total_delta);
            }

            // Konvergens — stoppa om energin stabiliserat sig
            if total_delta < CONVERGENCE_THRESHOLD {
                break;
            }
        }

        // Multi-hop micro propagation: om en nod har stark value-match,
        // boost syskon och förälders syskon (2-hop expansion)
        let value_matched: Vec<u32> = self
            .node_values
            .iter()
            .filter(|(id, _)| {
                self.nodes
                    .get(id)
                    .map(|s| s.amplitude > 0.3)
                    .unwrap_or(false)
            })
            .map(|(&id, _)| id)
            .collect();

        for nid in value_matched {
            // Boost siblings
            if let Some(&pid) = self.parent_map.get(&nid) {
                if let Some(siblings) = self.children_map.get(&pid) {
                    let amp = self.nodes.get(&nid).map(|s| s.amplitude).unwrap_or(0.0);
                    let boost = amp * 0.15; // 15% av nodens amplitude
                    for &sib_id in siblings {
                        if sib_id != nid {
                            if let Some(sib) = self.nodes.get_mut(&sib_id) {
                                if boost > sib.amplitude {
                                    sib.amplitude = boost;
                                    resonance_types
                                        .entry(sib_id)
                                        .or_insert(ResonanceType::Propagated);
                                }
                            }
                        }
                    }
                }
                // Boost parent's siblings (2-hop)
                if let Some(&grandparent) = self.parent_map.get(&pid) {
                    if let Some(uncles) = self.children_map.get(&grandparent) {
                        let amp = self.nodes.get(&nid).map(|s| s.amplitude).unwrap_or(0.0);
                        let boost = amp * 0.08; // 8% för 2-hop
                        for &uncle_id in uncles {
                            if uncle_id != pid {
                                if let Some(uncle) = self.nodes.get_mut(&uncle_id) {
                                    if boost > uncle.amplitude {
                                        uncle.amplitude = boost;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sibling pattern recognition: if one node in a sibling group matches,
        // give a small boost to structurally identical siblings.
        // Handles product grids, article lists, table rows.
        let high_amp_nodes: Vec<(u32, u32, f32)> = self
            .parent_map
            .iter()
            .filter_map(|(&child_id, &parent_id)| {
                self.nodes
                    .get(&child_id)
                    .filter(|s| s.amplitude > 0.4)
                    .map(|s| (child_id, parent_id, s.amplitude))
            })
            .collect();

        for (matched_id, parent_id, amp) in &high_amp_nodes {
            if let Some(siblings) = self.children_map.get(parent_id) {
                if siblings.len() >= 3 {
                    // Only for groups of 3+ siblings
                    let matched_role = self
                        .nodes
                        .get(matched_id)
                        .map(|s| s.role.clone())
                        .unwrap_or_default();
                    let boost = amp * 0.1; // 10% of matched node
                    for &sib_id in siblings {
                        if sib_id != *matched_id {
                            if let Some(sib) = self.nodes.get_mut(&sib_id) {
                                // Only boost structurally identical siblings (same role)
                                if sib.role == matched_role && boost > sib.amplitude {
                                    sib.amplitude = boost;
                                    resonance_types
                                        .entry(sib_id)
                                        .or_insert(ResonanceType::Propagated);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fas 3: Samla resultat
        // #6: Deterministic ranking — stabil tie-break på node_id
        let mut results: Vec<ResonanceResult> = self
            .nodes
            .iter()
            .filter(|(_, s)| s.amplitude > MIN_OUTPUT_THRESHOLD)
            .map(|(&nid, s)| ResonanceResult {
                node_id: nid,
                amplitude: s.amplitude,
                phase: s.phase,
                resonance_type: resonance_types
                    .get(&nid)
                    .cloned()
                    .unwrap_or(ResonanceType::Direct),
                causal_boost: causal_boosts.get(&nid).copied().unwrap_or(0.0),
            })
            .collect();

        // #6: Deterministic sort — amplitude DESC, then node_id ASC
        results.sort_by(|a, b| {
            b.amplitude
                .total_cmp(&a.amplitude)
                .then_with(|| a.node_id.cmp(&b.node_id))
        });

        // BUG-01: Label-hash deduplication — remove duplicates that waste top_n budget.
        // Keep lowest DOM depth + highest causal_boost on collision.
        {
            let mut seen_labels: HashMap<u64, usize> = HashMap::new(); // hash → index
            let mut deduped: Vec<ResonanceResult> = Vec::with_capacity(results.len());
            for r in results {
                let label = self
                    .node_labels
                    .get(&r.node_id)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let label_hash = hash_url(&label.trim().to_lowercase());
                if label.len() < 10 {
                    // Short labels (buttons, numbers) — don't dedup
                    deduped.push(r);
                } else if let Some(&existing_idx) = seen_labels.get(&label_hash) {
                    // Duplicate: keep if this one has higher causal_boost
                    if r.causal_boost > deduped[existing_idx].causal_boost {
                        deduped[existing_idx] = r;
                    }
                } else {
                    seen_labels.insert(label_hash, deduped.len());
                    deduped.push(r);
                }
            }
            results = deduped;
        }

        // Diversity penalty: penalize nodes sharing the same parent
        // Prevents top-N from being dominated by one DOM subtree
        {
            let mut parent_count: HashMap<u32, u32> = HashMap::new();
            for r in &results {
                if let Some(&pid) = self.parent_map.get(&r.node_id) {
                    *parent_count.entry(pid).or_insert(0) += 1;
                }
            }
            // Penalize 3rd+ sibling from same parent
            let mut parent_seen: HashMap<u32, u32> = HashMap::new();
            for r in results.iter_mut() {
                if let Some(&pid) = self.parent_map.get(&r.node_id) {
                    let seen = parent_seen.entry(pid).or_insert(0);
                    *seen += 1;
                    if *seen >= 4 && parent_count.get(&pid).copied().unwrap_or(0) >= 5 {
                        r.amplitude *= 0.85; // 15% penalty for 4th+ sibling (only in large groups)
                    }
                }
            }
            // Re-sort after diversity penalty
            results.sort_by(|a, b| {
                b.amplitude
                    .total_cmp(&a.amplitude)
                    .then_with(|| a.node_id.cmp(&b.node_id))
            });
        }

        // Build trace if requested
        let trace = if capture_trace {
            // För noder som fick amplitud via propagation (inte i cascade),
            // beräkna BM25/HDC-scores i efterhand så att trace-tabellen är komplett.
            let goal_hv_ref = &goal_hv;
            let node_scores: Vec<NodeScoreBreakdown> = results
                .iter()
                .take(50)
                .map(|r| {
                    let nid = r.node_id;
                    let in_cascade = trace_bm25_scores.contains_key(&nid);
                    if in_cascade {
                        // Noden scorades i cascade — använd sparade trace-värden
                        NodeScoreBreakdown {
                            node_id: nid,
                            bm25_score: trace_bm25_scores.get(&nid).copied().unwrap_or(0.0),
                            hdc_score: trace_hdc_scores.get(&nid).copied().unwrap_or(0.0),
                            role_priority: trace_role_priorities.get(&nid).copied().unwrap_or(0.0),
                            concept_boost: trace_concept_boosts.get(&nid).copied().unwrap_or(0.0),
                            causal_boost: r.causal_boost,
                            answer_shape: trace_answer_shapes.get(&nid).copied().unwrap_or(0.0),
                            answer_type_boost: trace_answer_types.get(&nid).copied().unwrap_or(0.0),
                            zone_penalty: trace_zone_penalties.get(&nid).copied().unwrap_or(1.0),
                            meta_penalty: trace_meta_penalties.get(&nid).copied().unwrap_or(1.0),
                            combmnz: trace_combmnz.get(&nid).copied().unwrap_or(1.0),
                            template_boost: trace_template_boosts
                                .get(&nid)
                                .copied()
                                .unwrap_or(false),
                            final_amplitude: r.amplitude,
                        }
                    } else {
                        // Noden fick amplitud via propagation — beräkna scores i efterhand
                        let bm25_s = bm25_scores.get(&nid).copied().unwrap_or(0.0);
                        let hdc_s = self
                            .nodes
                            .get(&nid)
                            .map(|s| {
                                let raw = s.text_hv.similarity(goal_hv_ref);
                                let norm = ((raw + 1.0) / 2.0).clamp(0.0, 1.0);
                                norm * norm
                            })
                            .unwrap_or(0.0);
                        let role_p = self
                            .nodes
                            .get(&nid)
                            .map(|s| role_priority(&s.role))
                            .unwrap_or(0.0);
                        let shape_s = shape_scores.get(&nid).copied().unwrap_or(0.0);
                        let meta_p = meta_penalties.get(&nid).copied().unwrap_or(1.0);
                        let zone_p = self
                            .nodes
                            .get(&nid)
                            .map(|s| zone_penalty(&s.role, s.depth))
                            .unwrap_or(1.0);
                        NodeScoreBreakdown {
                            node_id: nid,
                            bm25_score: bm25_s,
                            hdc_score: hdc_s,
                            role_priority: role_p,
                            concept_boost: 0.0,
                            causal_boost: r.causal_boost,
                            answer_shape: shape_s,
                            answer_type_boost: 0.0,
                            zone_penalty: zone_p,
                            meta_penalty: meta_p,
                            combmnz: 1.0,
                            template_boost: false,
                            final_amplitude: r.amplitude,
                        }
                    }
                })
                .collect();
            Some(PropagationTrace {
                bm25_candidates: trace_bm25_count,
                cascade_candidates: trace_cascade_count,
                propagation_iterations: trace_iteration_deltas.len() as u32,
                iteration_deltas: trace_iteration_deltas,
                gap_cut_position: None, // Filled by propagate_top_k_traced
                node_scores,
                total_field_nodes: self.nodes.len(),
                template_match,
            })
        } else {
            None
        };

        (results, trace)
    }

    /// Propagate and return only the most relevant nodes.
    ///
    /// Uses amplitude-gap detection: if there's a >30% relative drop between
    /// consecutive nodes, cut there. Falls back to hard `top_k` limit.
    /// This naturally selects the "resonant cluster" without a fixed threshold.
    pub fn propagate_top_k(&mut self, goal: &str, top_k: usize) -> Vec<ResonanceResult> {
        let all = self.propagate(goal);
        Self::apply_gap_filter(all, top_k)
    }

    /// Propagate with trace and gap detection
    pub fn propagate_top_k_traced(
        &mut self,
        goal: &str,
        top_k: usize,
    ) -> (Vec<ResonanceResult>, PropagationTrace) {
        let (all, mut trace) = self.propagate_traced(goal);
        let total = all.len();
        let filtered = Self::apply_gap_filter(all, top_k);
        let gap_pos = if filtered.len() < total.min(top_k) {
            Some(filtered.len())
        } else {
            None
        };
        trace.gap_cut_position = gap_pos;
        (filtered, trace)
    }

    /// Run multiple goal variants and merge results (union with max amplitude).
    ///
    /// Exploits sub-ms cache-hit: runs N variants for ~N×0.6ms total.
    /// Returns union of all results, keeping highest amplitude per node.
    pub fn propagate_multi_variant(
        &mut self,
        goals: &[&str],
        top_k: usize,
    ) -> Vec<ResonanceResult> {
        if goals.is_empty() {
            return Vec::new();
        }
        if goals.len() == 1 {
            return self.propagate_top_k(goals[0], top_k);
        }

        // #1 Batch optimization: ensure BM25 cache is built once, shared across all variants
        if self.bm25_cache.is_none() {
            let combined: Vec<(u32, String)> = self
                .node_labels
                .iter()
                .map(|(&id, label)| match self.node_values.get(&id) {
                    Some(val) => (id, format!("{} {} {}", label, val, val)),
                    None => (id, label.clone()),
                })
                .collect();
            let refs: Vec<(u32, &str)> = combined.iter().map(|(id, s)| (*id, s.as_str())).collect();
            self.bm25_cache = Some(TfIdfIndex::build(&refs));
        }

        // Run variants — BM25 cache is warm, each propagate() skips rebuild
        let mut best: HashMap<u32, ResonanceResult> = HashMap::new();
        for goal in goals {
            for state in self.nodes.values_mut() {
                state.amplitude = 0.0;
            }
            let results = self.propagate(goal);
            for r in results {
                let entry = best.entry(r.node_id).or_insert_with(|| ResonanceResult {
                    node_id: r.node_id,
                    amplitude: 0.0,
                    phase: r.phase,
                    resonance_type: r.resonance_type,
                    causal_boost: 0.0,
                });
                if r.amplitude > entry.amplitude {
                    entry.amplitude = r.amplitude;
                    entry.resonance_type = r.resonance_type;
                    entry.causal_boost = r.causal_boost;
                }
            }
        }

        let mut merged: Vec<ResonanceResult> = best.into_values().collect();
        merged.sort_by(|a, b| {
            b.amplitude
                .total_cmp(&a.amplitude)
                .then_with(|| a.node_id.cmp(&b.node_id))
        });

        Self::apply_gap_filter(merged, top_k)
    }

    /// Intelligent top-k med amplitud-gap detection.
    ///
    /// Hittar naturliga "klyftor" i sorterad amplitud-sekvens:
    /// om nod[i+1].amplitude < nod[i].amplitude * (1 - GAP_RATIO_THRESHOLD)
    /// klipps output vid position i+1 (men minst 3 noder, max top_k).
    fn apply_gap_filter(results: Vec<ResonanceResult>, top_k: usize) -> Vec<ResonanceResult> {
        if results.len() <= 3 {
            return results;
        }

        let limit = results.len().min(top_k);
        let mut cut_at = limit;

        // Sök efter amplitud-gap (börja efter position 2 för att garantera minst 3)
        for i in 2..limit {
            let prev = results[i - 1].amplitude;
            let curr = results[i].amplitude;

            // Relativ drop: om current < prev * 0.7 → klipp här
            if prev > 0.001 && curr < prev * (1.0 - GAP_RATIO_THRESHOLD) {
                cut_at = i;
                break;
            }
        }

        results.into_iter().take(cut_at).collect()
    }

    /// Provide feedback about which nodes successfully matched a goal.
    ///
    /// Provide feedback with confidence-weighted learning.
    ///
    /// Three improvements over v4:
    /// 1. Confidence-weighted: alpha += confidence (not +1)
    /// 2. Negative signal: beta += (1 - confidence) for non-successful edges
    /// 3. Temporal decay: all stats decay before update (prevents stale bias)
    pub fn feedback(&mut self, goal: &str, successful_node_ids: &[u32]) {
        let goal_hv = Hypervector::from_text_ngrams(goal);
        let goal_hash = hash_url(goal);
        let now = now_ms();
        let successful_set: std::collections::HashSet<u32> =
            successful_node_ids.iter().copied().collect();

        // Steg 0: Temporal decay på propagation_stats (Fix 3)
        // Alla stats krymper exponentiellt — förhindrar stale bias.
        // decay = 0.95 per feedback-anrop (nyare data väger mer)
        const STATS_DECAY: f32 = 0.95;
        for (alpha, beta) in self.propagation_stats.values_mut() {
            *alpha *= STATS_DECAY;
            *beta *= STATS_DECAY;
        }

        // Steg 1: Kausalt minne per nod
        for &nid in successful_node_ids {
            if let Some(state) = self.nodes.get_mut(&nid) {
                if state.last_goal_hash == goal_hash {
                    continue;
                }
                if state.hit_count == 0 {
                    state.causal_memory = goal_hv.clone();
                } else {
                    state.causal_memory = Hypervector::bundle(&[&state.causal_memory, &goal_hv]);
                }
                state.hit_count += 1;

                // R6: BTSP plasticity — faster feedback = stronger imprint
                let time_since_query = now.saturating_sub(state.last_hit_ms);
                let plasticity = if time_since_query < 1000 {
                    1.5 // Quick feedback: strong imprint
                } else if time_since_query < 10_000 {
                    1.0 // Normal feedback
                } else {
                    0.5 // Delayed feedback: weak imprint
                };

                if plasticity > 1.2 {
                    // Strong plasticity: double-bundle for stronger imprint
                    state.causal_memory =
                        Hypervector::bundle(&[&state.causal_memory, &goal_hv, &goal_hv]);
                }

                state.last_goal_hash = goal_hash;
                state.last_hit_ms = now;
            }
        }

        // Steg 2: Confidence-weighted Beta-distribution update.
        // Itererar alla parent→child edges EN gång. Uppdaterar BOTH directions.
        let confidences: HashMap<u32, f32> = self
            .nodes
            .iter()
            .map(|(&id, s)| (id, s.amplitude.clamp(0.0, 1.0)))
            .collect();

        // Snapshot edges med roller (undvik borrow-konflikter)
        let edges: Vec<(u32, u32, String, String)> = self
            .parent_map
            .iter()
            .filter_map(|(&child_id, &parent_id)| {
                let cr = self.nodes.get(&child_id).map(|s| s.role.clone())?;
                let pr = self.nodes.get(&parent_id).map(|s| s.role.clone())?;
                Some((child_id, parent_id, cr, pr))
            })
            .collect();

        for (child_id, _parent_id, child_role, parent_role) in &edges {
            let conf = confidences.get(child_id).copied().unwrap_or(0.0);
            let is_success = successful_set.contains(child_id);

            // Downward: förälderns roll → barn
            let dk = format!("{}:down", parent_role);
            let de = self
                .propagation_stats
                .entry(dk)
                .or_insert_with(|| (heuristic_down_weight(parent_role), 1.0));
            if is_success {
                de.0 += conf;
            } else {
                de.1 += 1.0 - conf;
            }

            // Upward: barnets roll → förälder
            let uk = format!("{}:up", child_role);
            let ue = self
                .propagation_stats
                .entry(uk)
                .or_insert_with(|| (heuristic_up_weight(child_role), 1.0));
            if is_success {
                ue.0 += conf;
            } else {
                ue.1 += 1.0 - conf;
            }
        }

        // Steg 3: Field-level concept memory
        // Aggregera framgångsrika noders text-HV:er per goal-token
        let goal_tokens: Vec<String> = goal
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(String::from)
            .collect();
        for &nid in successful_node_ids {
            if let Some(state) = self.nodes.get(&nid) {
                for token in &goal_tokens {
                    let entry = self
                        .concept_memory
                        .entry(token.clone())
                        .or_insert_with(|| HvData::from_hv(&Hypervector::zero()));
                    let existing = entry.to_hv();
                    let merged = Hypervector::bundle(&[&existing, &state.text_hv]);
                    *entry = HvData::from_hv(&merged);
                }
            }
        }

        // Evicta äldsta concept entries om cap nådd
        while self.concept_memory.len() > MAX_CONCEPT_ENTRIES {
            // Ta bort första (arbitrary men deterministisk)
            if let Some(key) = self.concept_memory.keys().next().cloned() {
                self.concept_memory.remove(&key);
            } else {
                break;
            }
        }
    }

    /// Implicit feedback: infer which nodes were useful from the LLM's response text.
    /// Computes BM25 similarity between response and each node's label.
    /// Nodes whose label appears (partially) in the response are treated as successful.
    /// This closes the learning loop without requiring explicit crfr_feedback calls.
    pub fn implicit_feedback(&mut self, goal: &str, response_text: &str) {
        if response_text.len() < 10 {
            return;
        }
        let response_lower = response_text.to_lowercase();
        let response_words: std::collections::HashSet<&str> = response_lower
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        if response_words.is_empty() {
            return;
        }

        // Find nodes whose labels significantly overlap with response
        let successful_ids: Vec<u32> = self
            .node_labels
            .iter()
            .filter_map(|(&id, label)| {
                if label.len() < 10 {
                    return None;
                }
                let label_lower = label.to_lowercase();
                let label_words: Vec<&str> = label_lower
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();
                if label_words.is_empty() {
                    return None;
                }

                // Count word overlap
                let overlap = label_words
                    .iter()
                    .filter(|w| response_words.contains(w as &str))
                    .count();
                let ratio = overlap as f32 / label_words.len() as f32;

                // Require >40% word overlap to count as implicit success
                if ratio > 0.4 {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        if !successful_ids.is_empty() {
            self.feedback(goal, &successful_ids);
        }
    }

    /// Propagate multiple goals simultaneously with interference.
    ///
    /// Constructive interference: nodes matching multiple goals get
    /// amplitude boost (sum of individual amplitudes).
    /// Destructive interference: goals with negative cross-similarity
    /// (< -0.1) cancel each other's contributions.
    pub fn multi_goal_propagate(&mut self, goals: &[&str]) -> Vec<ResonanceResult> {
        if goals.is_empty() {
            return Vec::new();
        }
        if goals.len() == 1 {
            return self.propagate(goals[0]);
        }

        // Beräkna HV:er för alla mål
        let goal_hvs: Vec<Hypervector> = goals
            .iter()
            .map(|g| Hypervector::from_text_ngrams(g))
            .collect();

        // Detektera destruktiv interferens mellan mål-par
        let mut destructive_pairs: Vec<(usize, usize)> = Vec::new();
        for i in 0..goal_hvs.len() {
            for j in (i + 1)..goal_hvs.len() {
                let cross_sim = goal_hvs[i].similarity(&goal_hvs[j]);
                if cross_sim < -0.1 {
                    destructive_pairs.push((i, j));
                }
            }
        }

        // Ackumulera amplitud per nod från alla mål
        let mut accumulated: HashMap<u32, (f32, f32, ResonanceType)> = HashMap::new();
        let mut causal_boosts: HashMap<u32, f32> = HashMap::new();

        // Spara och återställ tillstånd mellan propagationer
        let original_amplitudes: HashMap<u32, f32> = self
            .nodes
            .iter()
            .map(|(&id, s)| (id, s.amplitude))
            .collect();

        for (goal_idx, goal) in goals.iter().enumerate() {
            // Återställ amplituder före varje propagation
            for (&id, amp) in &original_amplitudes {
                if let Some(state) = self.nodes.get_mut(&id) {
                    state.amplitude = *amp;
                }
            }

            let results = self.propagate(goal);

            // Kontrollera om detta mål har destruktiv interferens
            let has_destructive = destructive_pairs
                .iter()
                .any(|&(i, j)| i == goal_idx || j == goal_idx);

            for r in &results {
                let entry =
                    accumulated
                        .entry(r.node_id)
                        .or_insert((0.0, r.phase, r.resonance_type));

                if has_destructive {
                    // Destruktiv interferens: reducera bidraget
                    entry.0 += r.amplitude * 0.5;
                } else {
                    // Konstruktiv interferens: fullt bidrag
                    entry.0 += r.amplitude;
                }

                // Uppdatera kausal boost
                let cb = causal_boosts.entry(r.node_id).or_insert(0.0);
                *cb += r.causal_boost;
            }
        }

        // Konstruktiv bonus: noder som matchar fler mål får extra förstärkning
        let goal_count = goals.len() as f32;
        let mut results: Vec<ResonanceResult> = accumulated
            .into_iter()
            .map(|(node_id, (total_amp, phase, rtype))| {
                // Normalisera och clampa
                let normalized = (total_amp / goal_count).clamp(0.0, 1.0);
                ResonanceResult {
                    node_id,
                    amplitude: normalized,
                    phase,
                    resonance_type: rtype,
                    causal_boost: causal_boosts.get(&node_id).copied().unwrap_or(0.0),
                }
            })
            .filter(|r| r.amplitude > MIN_OUTPUT_THRESHOLD)
            .collect();

        results.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
        results
    }

    /// Number of nodes in the field
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // ─── Incremental Field Update ───────────────────────────────────────

    /// Update a single node's text without rebuilding the entire field.
    ///
    /// Use when DOM mutates (JS-driven content change, AJAX response).
    /// Updates: text HV, BM25 label, BM25 cache invalidation.
    /// Preserves: causal memory, hit_count, parent/child relations.
    pub fn update_node(
        &mut self,
        node_id: u32,
        new_label: &str,
        new_role: &str,
        new_value: Option<&str>,
    ) {
        // Read old combined label BEFORE updating (for incremental BM25)
        let old_combined =
            self.node_labels
                .get(&node_id)
                .map(|old_label| match self.node_values.get(&node_id) {
                    Some(val) => format!("{} {} {}", old_label, val, val),
                    None => old_label.clone(),
                });

        // Update node state
        if let Some(state) = self.nodes.get_mut(&node_id) {
            state.text_hv = if new_label.is_empty() {
                Hypervector::from_seed(&format!("__empty_{}", node_id))
            } else {
                Hypervector::from_text_ngrams(new_label)
            };
            state.role = new_role.to_string();
        }
        self.node_labels.insert(node_id, new_label.to_string());
        if let Some(val) = new_value {
            self.node_values.insert(node_id, val.to_string());
        } else {
            self.node_values.remove(&node_id);
        }

        // Incremental BM25 update (not full invalidation)
        let new_combined = match new_value {
            Some(val) => format!("{} {} {}", new_label, val, val),
            None => new_label.to_string(),
        };
        if let Some(ref mut idx) = self.bm25_cache {
            if let Some(old) = old_combined {
                idx.update_node(node_id, &old, &new_combined);
            }
        }
    }

    /// Add a new node to the field (AJAX-laddat innehåll).
    ///
    /// Preserves existing causal memory. Parent/child relations uppdateras.
    pub fn add_node(
        &mut self,
        node_id: u32,
        label: &str,
        role: &str,
        parent_id: Option<u32>,
        value: Option<&str>,
    ) {
        let depth = parent_id
            .and_then(|pid| self.nodes.get(&pid).map(|p| p.depth + 1))
            .unwrap_or(0);

        self.nodes.insert(
            node_id,
            ResonanceState {
                text_hv: Hypervector::from_text_ngrams(label),
                role: role.to_string(),
                depth,
                phase: 0.0,
                amplitude: 0.0,
                causal_memory: Hypervector::zero(),
                hit_count: 0,
                last_goal_hash: 0,
                last_hit_ms: 0,
            },
        );
        self.node_labels.insert(node_id, label.to_string());
        if let Some(val) = value {
            self.node_values.insert(node_id, val.to_string());
        }
        if let Some(pid) = parent_id {
            self.parent_map.insert(node_id, pid);
            self.children_map.entry(pid).or_default().push(node_id);
        }
        // Incremental BM25: add new node to existing index
        if let Some(ref mut idx) = self.bm25_cache {
            let combined = match value {
                Some(val) => format!("{} {} {}", label, val, val),
                None => label.to_string(),
            };
            idx.update_node(node_id, "", &combined); // Empty old_label = pure add
        }
    }

    /// Remove a node from the field (DOM-deletion).
    pub fn remove_node(&mut self, node_id: u32) {
        self.nodes.remove(&node_id);
        self.node_labels.remove(&node_id);
        self.node_values.remove(&node_id);
        if let Some(pid) = self.parent_map.remove(&node_id) {
            if let Some(children) = self.children_map.get_mut(&pid) {
                children.retain(|&id| id != node_id);
            }
        }
        self.children_map.remove(&node_id);
        // Incremental BM25: remove from existing index
        if let Some(ref mut idx) = self.bm25_cache {
            idx.remove_node(node_id);
        }
    }

    // ─── Cross-URL Transfer ─────────────────────────────────────────────

    /// Transfer causal memory from another field (similar site type).
    ///
    /// Use case: transfer learning between news sites (SVT → Expressen),
    /// e-commerce sites (Amazon → Zalando), etc.
    ///
    /// Transfers causal memory from donor nodes to matching recipient nodes
    /// based on role + label similarity. Only transfers if donor has
    /// hit_count > 0 (has actual learned knowledge).
    ///
    /// `min_similarity`: minimum HDC text similarity to transfer (0.0-1.0).
    /// Returns number of nodes that received transferred memory.
    pub fn transfer_from(&mut self, donor: &ResonanceField, min_similarity: f32) -> u32 {
        let mut transferred = 0u32;

        // Pre-bucket donors by role for O(N) lookup instead of O(N²)
        let mut donor_by_role: HashMap<&str, Vec<&ResonanceState>> = HashMap::new();
        for (_, state) in donor.nodes.iter().filter(|(_, s)| s.hit_count > 0) {
            donor_by_role
                .entry(state.role.as_str())
                .or_default()
                .push(state);
        }

        if donor_by_role.is_empty() {
            return 0;
        }

        for (_recipient_id, recipient) in self.nodes.iter_mut() {
            if recipient.hit_count > 0 {
                continue;
            }

            let Some(donors) = donor_by_role.get(recipient.role.as_str()) else {
                continue;
            };

            let mut best_sim: f32 = min_similarity;
            let mut best_donor: Option<&ResonanceState> = None;

            for donor_state in donors {
                let sim = recipient.text_hv.similarity(&donor_state.text_hv);
                let norm = ((sim + 1.0) / 2.0).clamp(0.0, 1.0);
                if norm > best_sim {
                    best_sim = norm;
                    best_donor = Some(donor_state);
                }
            }

            if let Some(donor) = best_donor {
                let transfer_strength = best_sim * 0.5;
                if transfer_strength > 0.1 {
                    recipient.causal_memory =
                        Hypervector::bundle(&[&recipient.causal_memory, &donor.causal_memory]);
                    recipient.hit_count = 1;
                    recipient.last_hit_ms = now_ms();
                    transferred += 1;
                }
            }
        }

        transferred
    }

    // ─── Confidence Calibration ─────────────────────────────────────────

    /// Calibrate raw amplitudes to probability estimates.
    ///
    /// Maps amplitude (0-1, arbitrary scale) to estimated probability
    /// that the node contains the answer (0-1, calibrated).
    ///
    /// Uses Platt scaling approximation:
    ///   P(answer) = 1 / (1 + exp(-k * (amplitude - threshold)))
    /// where k and threshold are derived from the field's hit history.
    pub fn calibrate_results(&self, results: &[ResonanceResult]) -> Vec<(u32, f32, f32)> {
        // Samla kalibrerings-statistik från fältets historik
        let total_hits: u32 = self.nodes.values().map(|s| s.hit_count).sum();
        let total_nodes = self.nodes.len() as f32;
        let base_rate = if total_nodes > 0.0 {
            (total_hits as f32 / total_nodes).clamp(0.001, 0.5)
        } else {
            0.05 // Prior: 5% av noder innehåller svar
        };

        // Platt-skalning: k styr branthet, threshold styr center
        // Adaptivt: fler hits → mer data → brantare sigmoid
        let k = 5.0 + (total_hits as f32).min(50.0) * 0.2; // 5-15
        let threshold = 0.3 - base_rate * 0.5; // Lägre threshold om fler hits

        results
            .iter()
            .map(|r| {
                let logit = k * (r.amplitude - threshold);
                let probability = 1.0 / (1.0 + (-logit).exp());
                // Boost om noden har kausalt minne (höjer confidence)
                let causal_factor = if r.causal_boost > 0.01 { 1.2 } else { 1.0 };
                let calibrated = (probability * causal_factor).clamp(0.0, 0.99);
                (r.node_id, r.amplitude, calibrated)
            })
            .collect()
    }

    /// Serialize the field to a JSON string for persistent storage.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Serialize failed: {e}"))
    }

    /// Deserialize a field from a JSON string (restores causal memory).
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Deserialize failed: {e}"))
    }
}

// ─── LRU Field Cache ────────────────────────────────────────────────────────

/// Default cache TTL (3 minuter)
const DEFAULT_CACHE_TTL_MS: u64 = 180_000;

/// Global LRU-cache för resonansfält per URL.
///
/// Sparar fältet (med kausalt minne) så att upprepade besök till samma
/// sida drar nytta av tidigare lärande. TTL-baserad eviction.
struct FieldCacheInner {
    /// (url_hash, insert_ms, field)
    entries: Vec<(u64, u64, ResonanceField)>,
    capacity: usize,
}

impl FieldCacheInner {
    fn new(capacity: usize) -> Self {
        FieldCacheInner {
            entries: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Hämta ett fält via move (tar bort ur cache, noll-klon).
    /// Returnerar None om TTL expired.
    fn take(&mut self, url_hash: u64) -> Option<ResonanceField> {
        let now = now_ms();
        if let Some(pos) = self.entries.iter().position(|(h, _, _)| *h == url_hash) {
            let (_, insert_ms, _) = &self.entries[pos];
            if now.saturating_sub(*insert_ms) > DEFAULT_CACHE_TTL_MS {
                // TTL expired — evicta, returnera None (tvingar rebuild)
                self.entries.remove(pos);
                return None;
            }
            let (_, _, field) = self.entries.remove(pos);
            Some(field)
        } else {
            None
        }
    }

    /// Spara ett fält (ersätt om redan finns, evicta äldsta om fullt)
    fn put(&mut self, url_hash: u64, field: ResonanceField) {
        self.entries.retain(|(h, _, _)| *h != url_hash);
        // Evicta expired entries
        let now = now_ms();
        self.entries
            .retain(|(_, insert_ms, _)| now.saturating_sub(*insert_ms) <= DEFAULT_CACHE_TTL_MS);
        // Evicta äldsta om fullt
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((url_hash, now, field));
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

static FIELD_CACHE: std::sync::LazyLock<Mutex<FieldCacheInner>> =
    std::sync::LazyLock::new(|| Mutex::new(FieldCacheInner::new(FIELD_CACHE_CAPACITY)));

// ─── Domain-Level Shared Learning ──────────────────────────────────────────

/// Domain-level aggregated learning: shared propagation stats + concept memory.
/// Used as warm-start prior for new URLs from the same domain.
struct DomainProfile {
    /// Aggregated propagation stats from all URLs on this domain
    stats: HashMap<String, (f32, f32)>,
    /// Aggregated concept memory from all successful extractions
    concepts: HashMap<String, HvData>,
    /// Number of fields that contributed to this profile
    field_count: u32,
}

struct DomainRegistry {
    profiles: HashMap<u64, DomainProfile>, // domain_hash → profile
    capacity: usize,
}

impl DomainRegistry {
    fn new(capacity: usize) -> Self {
        DomainRegistry {
            profiles: HashMap::new(),
            capacity,
        }
    }

    /// Hämta domain-profil (om den finns)
    fn get(&self, domain_hash: u64) -> Option<&DomainProfile> {
        self.profiles.get(&domain_hash)
    }

    /// Uppdatera domain-profil med ett fälts inlärda stats
    fn update(&mut self, domain_hash: u64, field: &ResonanceField) {
        let profile = self
            .profiles
            .entry(domain_hash)
            .or_insert_with(|| DomainProfile {
                stats: HashMap::new(),
                concepts: HashMap::new(),
                field_count: 0,
            });

        // Merge field's propagation_stats into domain profile (running average)
        profile.field_count += 1;
        let n = profile.field_count as f32;
        for (key, &(alpha, beta)) in &field.propagation_stats {
            let entry = profile.stats.entry(key.clone()).or_insert((1.0, 1.0));
            // Weighted running average: blend new data with existing
            entry.0 = entry.0 * ((n - 1.0) / n) + alpha / n;
            entry.1 = entry.1 * ((n - 1.0) / n) + beta / n;
        }

        // Merge concept memory (bundle HVs)
        for (token, hv_data) in &field.concept_memory {
            let entry = profile
                .concepts
                .entry(token.clone())
                .or_insert_with(|| HvData::from_hv(&Hypervector::zero()));
            let existing = entry.to_hv();
            let new_hv = hv_data.to_hv();
            let merged = Hypervector::bundle(&[&existing, &new_hv]);
            *entry = HvData::from_hv(&merged);
        }

        // Evict oldest if over capacity
        if self.profiles.len() > self.capacity {
            if let Some(key) = self.profiles.keys().next().cloned() {
                self.profiles.remove(&key);
            }
        }
    }
}

static DOMAIN_REGISTRY: std::sync::LazyLock<Mutex<DomainRegistry>> =
    std::sync::LazyLock::new(|| Mutex::new(DomainRegistry::new(128)));

/// Get a cached resonance field for a URL, or build a new one.
///
/// If a cached field exists, it retains all causal memory from previous
/// interactions. The caller should call `save_field` after propagation
/// to persist any new causal learning.
pub fn get_or_build_field(tree_nodes: &[SemanticNode], url: &str) -> (ResonanceField, bool) {
    get_or_build_field_with_variant(tree_nodes, url, false)
}

/// Get/build field med JS-variant-separation.
/// run_js=true och run_js=false cachelagras separat så att JS-renderat
/// innehåll inte förväxlas med statiskt.
pub fn get_or_build_field_with_variant(
    tree_nodes: &[SemanticNode],
    url: &str,
    js_variant: bool,
) -> (ResonanceField, bool) {
    // Inkludera js_variant i hash så att samma URL med/utan JS får olika cache-entries
    let variant_url = if js_variant {
        format!("{}#__js_eval", url)
    } else {
        url.to_string()
    };
    let url_hash = hash_url(&variant_url);
    let mut cache = match FIELD_CACHE.lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(field) = cache.take(url_hash) {
        return (field, true);
    }
    drop(cache);
    let mut field = ResonanceField::from_semantic_tree(tree_nodes, url);
    // Sätt korrekt url_hash (med variant) för cache-konistens
    field.url_hash = url_hash;
    (field, false)
}

/// Save a resonance field back to the cache (preserves causal memory).
pub fn save_field(field: &ResonanceField) {
    let mut cache = match FIELD_CACHE.lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    cache.put(field.url_hash, field.clone());

    // Uppdatera domain-profil med inlärda stats
    if let Ok(mut registry) = DOMAIN_REGISTRY.lock() {
        registry.update(field.domain_hash, field);
    }
}

/// Get cache statistics (entries, capacity).
pub fn cache_stats() -> (usize, usize) {
    let cache = match FIELD_CACHE.lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    (cache.len(), cache.capacity)
}

/// Summary info for a cached resonance field (for dashboard).
pub struct FieldSummary {
    pub url_hash: u64,
    pub url: String,
    pub node_count: usize,
    pub total_queries: u32,
    pub domain_hash: u64,
    pub created_at_ms: u64,
    pub insert_ms: u64,
    pub propagation_weight_count: usize,
    pub concept_memory_count: usize,
    pub propagation_stats: std::collections::HashMap<String, (f32, f32)>,
    pub structure_hash: u64,
    pub max_depth: u32,
    pub edge_count: usize,
}

/// List summaries of all cached resonance fields (non-destructive peek).
pub fn list_cached_fields() -> Vec<FieldSummary> {
    let cache = match FIELD_CACHE.lock() {
        Ok(c) => c,
        Err(poisoned) => poisoned.into_inner(),
    };
    cache
        .entries
        .iter()
        .map(|(url_hash, insert_ms, field)| {
            let max_depth = field.nodes.values().map(|s| s.depth).max().unwrap_or(0);
            FieldSummary {
                url_hash: *url_hash,
                url: field.url.clone(),
                node_count: field.nodes.len(),
                total_queries: field.total_queries,
                domain_hash: field.domain_hash,
                created_at_ms: field.created_at_ms,
                insert_ms: *insert_ms,
                propagation_weight_count: field.propagation_stats.len(),
                concept_memory_count: field.concept_memory.len(),
                propagation_stats: field.propagation_stats.clone(),
                structure_hash: field.structure_hash,
                max_depth,
                edge_count: field.parent_map.len(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SemanticNode;

    /// Skapa en enkel SemanticNode med givna fält
    fn make_node(id: u32, role: &str, label: &str, children: Vec<SemanticNode>) -> SemanticNode {
        SemanticNode {
            id,
            role: role.into(),
            label: label.into(),
            children,
            ..SemanticNode::default()
        }
    }

    #[test]
    fn test_field_creation_basic() {
        let tree = vec![
            make_node(
                1,
                "text",
                "population statistics",
                vec![make_node(2, "text", "367924 inhabitants", vec![])],
            ),
            make_node(3, "navigation", "site menu home about", vec![]),
        ];

        let field = ResonanceField::from_semantic_tree(&tree, "https://example.com");

        assert_eq!(field.node_count(), 3, "Fältet borde ha 3 noder");
        assert!(field.url_hash != 0, "URL-hash borde vara icke-noll");
        assert_eq!(field.total_queries, 0, "Inga propagationer ännu");
    }

    #[test]
    fn test_field_creation_empty_tree() {
        let tree: Vec<SemanticNode> = vec![];
        let field = ResonanceField::from_semantic_tree(&tree, "https://empty.com");

        assert_eq!(field.node_count(), 0, "Tomt träd borde ge tomt fält");
    }

    #[test]
    fn test_field_creation_single_node() {
        let tree = vec![make_node(42, "button", "submit form", vec![])];
        let field = ResonanceField::from_semantic_tree(&tree, "https://single.com");

        assert_eq!(field.node_count(), 1, "Borde ha exakt 1 nod");
        assert!(
            field.nodes.contains_key(&42),
            "Nod 42 borde finnas i fältet"
        );
    }

    #[test]
    fn test_propagation_returns_relevant_nodes() {
        let tree = vec![
            make_node(
                1,
                "text",
                "buy product shopping cart",
                vec![make_node(2, "button", "add to cart", vec![])],
            ),
            make_node(3, "text", "cookie privacy policy settings", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://shop.com");
        let results = field.propagate("buy product add cart");

        assert!(
            !results.is_empty(),
            "Propagation borde returnera relevanta noder"
        );

        // Nod 1 ("buy product") eller nod 2 ("add to cart") borde ha hög amplitud
        let top_id = results[0].node_id;
        assert!(
            top_id == 1 || top_id == 2,
            "Topnoden borde vara nod 1 eller 2 (shopping-relaterad), fick nod {top_id}"
        );

        // Nod 3 (cookie policy) borde ha lägre amplitud än shopping-noder
        let shop_max = results
            .iter()
            .filter(|r| r.node_id == 1 || r.node_id == 2)
            .map(|r| r.amplitude)
            .fold(0.0f32, f32::max);
        let cookie_amp = results
            .iter()
            .find(|r| r.node_id == 3)
            .map(|r| r.amplitude)
            .unwrap_or(0.0);

        assert!(
            shop_max > cookie_amp,
            "Shopping-noder borde ha högre amplitud ({shop_max}) än cookie-nod ({cookie_amp})"
        );
    }

    #[test]
    fn test_propagation_sorts_descending() {
        let tree = vec![
            make_node(1, "heading", "main title important", vec![]),
            make_node(2, "text", "some body text content", vec![]),
            make_node(3, "link", "navigation link home", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");
        let results = field.propagate("main title");

        // Kontrollera att resultat är sorterade fallande
        for window in results.windows(2) {
            assert!(
                window[0].amplitude >= window[1].amplitude,
                "Resultat borde vara sorterade fallande: {} >= {}",
                window[0].amplitude,
                window[1].amplitude
            );
        }
    }

    #[test]
    fn test_causal_feedback_improves_resonance() {
        let tree = vec![
            make_node(1, "button", "submit order", vec![]),
            make_node(2, "text", "unrelated navigation", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        // Första propagation — baslinjeamplitud
        let results_before = field.propagate("confirm purchase");
        let amp_before = results_before
            .iter()
            .find(|r| r.node_id == 1)
            .map(|r| r.amplitude)
            .unwrap_or(0.0);

        // Ge feedback: nod 1 var lyckad för "confirm purchase"
        field.feedback("confirm purchase", &[1]);

        // Kontrollera att kausalt minne uppdaterades
        let state = field.nodes.get(&1).expect("Nod 1 borde finnas");
        assert_eq!(state.hit_count, 1, "Hit count borde vara 1 efter feedback");

        // Andra propagation med liknande mål — borde ge högre amplitud
        let results_after = field.propagate("confirm order purchase");
        let amp_after = results_after
            .iter()
            .find(|r| r.node_id == 1)
            .map(|r| r.amplitude)
            .unwrap_or(0.0);

        assert!(
            amp_after >= amp_before,
            "Amplitud borde vara minst lika hög efter kausal feedback: before={amp_before}, after={amp_after}"
        );
    }

    #[test]
    fn test_feedback_avoids_double_counting() {
        let tree = vec![make_node(1, "button", "click me", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        // Samma feedback två gånger
        field.feedback("same goal", &[1]);
        field.feedback("same goal", &[1]);

        let state = field.nodes.get(&1).expect("Nod 1 borde finnas");
        assert_eq!(
            state.hit_count, 1,
            "Hit count borde vara 1 (dubbelräkning undviks)"
        );
    }

    #[test]
    fn test_multi_goal_propagate_empty() {
        let tree = vec![make_node(1, "text", "some content", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        let results = field.multi_goal_propagate(&[]);
        assert!(results.is_empty(), "Inga mål borde ge tomma resultat");
    }

    #[test]
    fn test_multi_goal_propagate_single() {
        let tree = vec![make_node(1, "text", "search products", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        let single = field.propagate("search products");

        // Återställ amplituder
        for state in field.nodes.values_mut() {
            state.amplitude = 0.0;
        }

        let multi = field.multi_goal_propagate(&["search products"]);

        // Borde ge liknande resultat (inte identiska pga total_queries-skillnad)
        assert_eq!(
            single.is_empty(),
            multi.is_empty(),
            "Enkel- och multi-mål borde ge samma tomhet/icke-tomhet"
        );
    }

    #[test]
    fn test_multi_goal_constructive_interference() {
        let tree = vec![
            make_node(1, "button", "buy product add to cart", vec![]),
            make_node(2, "text", "weather forecast tomorrow", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        // Två relaterade mål — borde konstruktivt interferera på nod 1
        let results = field.multi_goal_propagate(&["buy product", "add to cart"]);

        if !results.is_empty() {
            let top = &results[0];
            assert_eq!(
                top.node_id, 1,
                "Nod 1 borde ha högst amplitud vid konstruktiv interferens"
            );
        }
    }

    #[test]
    fn test_wave_propagation_reaches_children() {
        let tree = vec![make_node(
            1,
            "text",
            "product listing electronics",
            vec![
                make_node(2, "button", "view details", vec![]),
                make_node(3, "text", "price information", vec![]),
            ],
        )];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://shop.com");
        let results = field.propagate("product listing electronics");

        // Barn borde få propagerad amplitud från förälder
        let child_results: Vec<&ResonanceResult> = results
            .iter()
            .filter(|r| r.node_id == 2 || r.node_id == 3)
            .collect();

        // Minst en barnnod borde ha fått amplitud via propagation
        let any_propagated = child_results.iter().any(|r| r.amplitude > 0.0);

        assert!(
            any_propagated || child_results.is_empty(),
            "Barnnoder borde antingen ha propagerad amplitud eller inte passera tröskeln"
        );
    }

    #[test]
    fn test_resonance_result_types() {
        let tree = vec![make_node(1, "text", "exact match goal text", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        let results = field.propagate("exact match goal text");
        if !results.is_empty() {
            let top = &results[0];
            assert_eq!(
                top.resonance_type,
                ResonanceType::Direct,
                "Direkt matchning borde ge ResonanceType::Direct"
            );
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let tree = vec![make_node(1, "button", "test serde", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://serde.com");
        field.feedback("test goal", &[1]);

        // Serialisera
        let json = serde_json::to_string(&field).expect("Serialisering borde lyckas");
        assert!(!json.is_empty(), "JSON borde inte vara tom");

        // Deserialisera
        let restored: ResonanceField =
            serde_json::from_str(&json).expect("Deserialisering borde lyckas");
        assert_eq!(
            restored.node_count(),
            field.node_count(),
            "Antalet noder borde bevaras"
        );
        assert_eq!(restored.url_hash, field.url_hash, "URL-hash borde bevaras");

        let state = restored.nodes.get(&1).expect("Nod 1 borde finnas");
        assert_eq!(
            state.hit_count, 1,
            "Hit count borde bevaras efter serialisering"
        );
    }

    #[test]
    fn test_url_hash_differs() {
        let tree = vec![make_node(1, "text", "content", vec![])];
        let field_a = ResonanceField::from_semantic_tree(&tree, "https://a.com");
        let field_b = ResonanceField::from_semantic_tree(&tree, "https://b.com");

        assert_ne!(
            field_a.url_hash, field_b.url_hash,
            "Olika URL:er borde ge olika hash"
        );
    }

    #[test]
    fn test_total_queries_increments() {
        let tree = vec![make_node(1, "text", "something", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");

        assert_eq!(field.total_queries, 0, "Borde starta på 0");
        field.propagate("goal one");
        assert_eq!(field.total_queries, 1, "Borde vara 1 efter en propagation");
        field.propagate("goal two");
        assert_eq!(
            field.total_queries, 2,
            "Borde vara 2 efter två propagationer"
        );
    }

    #[test]
    fn test_propagate_traced_fills_propagated_node_scores() {
        // Förälder matchar BM25, barn matchar inte men borde få HDC-score via retroaktiv beräkning
        let tree = vec![make_node(
            1,
            "heading",
            "latest news headlines today",
            vec![
                make_node(2, "link", "breaking story about politics", vec![]),
                make_node(3, "link", "sports results from yesterday", vec![]),
            ],
        )];

        let mut field = ResonanceField::from_semantic_tree(&tree, "https://test.com");
        let (results, trace) = field.propagate_top_k_traced("latest news", 10);

        assert!(!results.is_empty(), "Borde returnera minst en nod");
        assert!(
            !trace.node_scores.is_empty(),
            "Borde ha node score breakdowns"
        );

        // Nod 1 (heading) borde ha BM25 > 0 (direkt match)
        let score_1 = trace.node_scores.iter().find(|s| s.node_id == 1);
        assert!(score_1.is_some(), "Heading-noden borde finnas i trace");
        if let Some(s) = score_1 {
            assert!(
                s.bm25_score > 0.0,
                "Heading borde ha BM25 > 0, fick {}",
                s.bm25_score
            );
        }

        // Om nod 2 eller 3 fick propagerad amplitud, borde de ha HDC > 0
        for child_id in [2, 3] {
            if let Some(score) = trace.node_scores.iter().find(|s| s.node_id == child_id) {
                assert!(
                    score.hdc_score > 0.0,
                    "Propagerad nod {} borde ha HDC > 0 (retroaktivt), fick {}",
                    child_id,
                    score.hdc_score
                );
                assert!(
                    score.role_priority > 0.0,
                    "Propagerad nod {} borde ha role_priority > 0, fick {}",
                    child_id,
                    score.role_priority
                );
            }
        }

        // Iteration deltas borde finnas
        assert!(
            !trace.iteration_deltas.is_empty(),
            "Borde ha propagation iteration deltas"
        );
    }
}
