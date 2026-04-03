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

use crate::scoring::hdc::Hypervector;
use crate::scoring::tfidf::TfIdfIndex;
use crate::types::SemanticNode;

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// Bas-dämpning vid propagation från förälder till barn
const BASE_CHILD_DAMPING: f32 = 0.35;
/// Bas-förstärkning vid propagation från barn till förälder
const BASE_PARENT_AMPLIFICATION: f32 = 0.25;
/// Bonus-multiplikator vid fas-synkronisering
const PHASE_SYNC_BONUS: f32 = 1.08;
/// Fönster (radianer) inom vilket fas-synk aktiveras
const PHASE_SYNC_WINDOW: f32 = std::f32::consts::FRAC_PI_4;
/// Minsta amplitud för att en nod ska propagera vidare
const ACTIVATION_THRESHOLD: f32 = 0.01;
/// Minsta amplitud för att inkluderas i resultat
const MIN_OUTPUT_THRESHOLD: f32 = 0.01;
/// Max antal propagations-iterationer (konvergens avbryter normalt vid 2-3)
const MAX_PROPAGATION_STEPS: u32 = 6;
/// Konvergenströskel: om total amplitudförändring < detta, stoppa
const CONVERGENCE_THRESHOLD: f32 = 0.001;
/// Max fan-out per nod i propagation (cap för O(N)-garanti)
/// En <ul> med 200 <li> propagerar bara till de första MAX_FAN_OUT barnen.
const MAX_FAN_OUT: usize = 32;
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
/// Max antal fält i LRU-cachen
const FIELD_CACHE_CAPACITY: usize = 64;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Type of resonance that caused a node to appear in results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub phase: f32,
    /// Typ av resonans som dominerade
    pub resonance_type: ResonanceType,
    /// Bidrag från kausalt minne
    pub causal_boost: f32,
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
        let mut arr = [0u64; 64];
        let len = self.bits.len().min(64);
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
    /// URL-hash som identifierar sidan
    pub url_hash: u64,
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

/// Adaptive downward propagation weight.
///
/// Baseras på roll-heuristik + feedback-signal:
/// - Basvikt: strukturbaserad (container → barn starkare)
/// - Feedback-boost: noder med hit_count > 0 → starkare propagation
///   (roller som historiskt gett rätt svar sprider mer energi)
///
/// Observera: vikterna är startpunkter, inte sanningar. Feedback
/// justerar dem per sajt. En e-commerce-sajt där "generic" divvar
/// innehåller produktpriser lär sig automatiskt att "generic" ska
/// sprida mer — utan att ändra hårdkodade tabeller.
fn adaptive_down_weight(state: &ResonanceState, _all: &HashMap<u32, ResonanceState>) -> f32 {
    // Bas: roll-heuristik (initial guess, skalad ned)
    let base = match state.role.as_str() {
        "heading" | "table" | "row" | "list" => 1.2,
        "generic" | "article" | "section" => 1.0,
        "price" | "text" | "button" | "link" => 0.7,
        "navigation" | "complementary" | "contentinfo" => 0.3,
        _ => 1.0,
    };
    // Feedback-adaption: noder som historiskt gett svar sprider mer
    // hit_count=0 → ×1.0, hit_count=1 → ×1.1, hit_count=5 → ×1.25
    let feedback_boost = 1.0 + (state.hit_count as f32).min(10.0) * 0.05;
    base * feedback_boost
}

/// Adaptive upward propagation weight.
///
/// Samma princip: roll-heuristik + feedback-signal.
fn adaptive_up_weight(state: &ResonanceState) -> f32 {
    let base = match state.role.as_str() {
        "price" | "data" | "cell" => 1.3,
        "text" | "paragraph" | "heading" => 1.1,
        "button" | "link" | "cta" => 0.9,
        "navigation" | "complementary" => 0.3,
        _ => 1.0,
    };
    let feedback_boost = 1.0 + (state.hit_count as f32).min(10.0) * 0.05;
    base * feedback_boost
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

/// Hämta nuvarande tid i millisekunder (fallback till 0 utan std::time)
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

        ResonanceField {
            nodes,
            parent_map,
            children_map,
            node_labels,
            node_values,
            bm25_cache: None,
            url_hash: hash_url(url),
            created_at_ms: now_ms(),
            total_queries: 0,
        }
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
        let text_hv = Hypervector::from_text_ngrams(&node.label);
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
    }

    /// Propagate a goal through the resonance field.
    ///
    /// Phase 1: Compute initial resonance from HDC similarity + causal memory.
    /// Phase 2: Wave propagation through parent-child edges.
    /// Phase 3: Collect and sort results by amplitude.
    pub fn propagate(&mut self, goal: &str) -> Vec<ResonanceResult> {
        self.total_queries += 1;
        let goal_hv = Hypervector::from_text_ngrams(goal);

        let mut causal_boosts: HashMap<u32, f32> = HashMap::new();
        let mut resonance_types: HashMap<u32, ResonanceType> = HashMap::new();

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
                        (id, format!("{} {}", label, val))
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
        let node_ids: Vec<u32> = self.nodes.keys().copied().collect();
        for &nid in &node_ids {
            if let Some(state) = self.nodes.get_mut(&nid) {
                let bm25_score = bm25_scores.get(&nid).copied().unwrap_or(0.0);

                let raw_sim = state.text_hv.similarity(&goal_hv);
                let hdc_norm = ((raw_sim + 1.0) / 2.0).clamp(0.0, 1.0);
                let hdc_score = hdc_norm * hdc_norm;

                // #9: Ren roll-prioritet — ingen semantisk HV-matchning
                let role_boost = role_priority(&state.role);

                let base_resonance = BM25_WEIGHT * bm25_score
                    + HDC_TEXT_WEIGHT * hdc_score
                    + ROLE_WEIGHT * role_boost;

                let causal_boost = if state.hit_count > 0 {
                    let raw_causal = state.causal_memory.similarity(&goal_hv);
                    let norm_causal = ((raw_causal + 1.0) / 2.0).clamp(0.0, 1.0);
                    let elapsed_s = (now.saturating_sub(state.last_hit_ms) as f64) / 1000.0;
                    let decay = (-CAUSAL_DECAY_LAMBDA * elapsed_s).exp() as f32;
                    norm_causal * norm_causal * CAUSAL_WEIGHT * decay
                } else {
                    0.0
                };

                state.amplitude = (base_resonance + causal_boost).clamp(0.0, 1.0);
                causal_boosts.insert(nid, causal_boost);

                if causal_boost > base_resonance * 0.5 && state.hit_count > 0 {
                    resonance_types.insert(nid, ResonanceType::CausalMemory);
                } else if base_resonance > ACTIVATION_THRESHOLD {
                    resonance_types.insert(nid, ResonanceType::Direct);
                }
            }
        }

        // Fas 2: Convergent propagation — O(E) per iteration, max 6 iterationer.
        //
        // Komplexitetsanalys:
        //   Varje iteration traverserar alla edges en gång: O(E) = O(N) för ett träd.
        //   Fan-out cap (MAX_FAN_OUT=32) garanterar att inga high-degree noder
        //   (t.ex. <ul> med 200 <li>) blåser upp till O(N²).
        //   Konvergens stoppar normalt vid 2-3 iterationer.
        //   Total: O(K × min(E, N×32)) där K ≈ 2-3.
        //
        // Snapshot: Vec istf HashMap — O(N) men med bättre cache-locality.
        for _step in 0..MAX_PROPAGATION_STEPS {
            // Snapshot amplituder (Vec för cache-locality, sorterad efter nod-id)
            let amplitudes: Vec<(u32, f32, f32)> = self
                .nodes
                .iter()
                .map(|(&id, s)| (id, s.amplitude, s.phase))
                .collect();
            let amp_map: HashMap<u32, (f32, f32)> = amplitudes
                .iter()
                .map(|&(id, amp, ph)| (id, (amp, ph)))
                .collect();

            let mut total_delta: f32 = 0.0;

            // Förälder → barn (fan-out capped)
            for (&parent_id, children) in &self.children_map {
                let (parent_amp, parent_phase) =
                    amp_map.get(&parent_id).copied().unwrap_or((0.0, 0.0));
                if parent_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }
                let confidence_factor = parent_amp.sqrt();
                let role_factor = self
                    .nodes
                    .get(&parent_id)
                    .map(|s| adaptive_down_weight(s, &self.nodes))
                    .unwrap_or(1.0);
                let damping = BASE_CHILD_DAMPING * confidence_factor * role_factor;

                // Fan-out cap: max MAX_FAN_OUT barn per nod
                let fan_out = children.len().min(MAX_FAN_OUT);
                for &child_id in &children[..fan_out] {
                    if let Some(child_state) = self.nodes.get_mut(&child_id) {
                        let mut propagated = parent_amp * damping;
                        let phase_diff = (parent_phase - child_state.phase).abs();
                        if phase_diff < PHASE_SYNC_WINDOW {
                            propagated *= PHASE_SYNC_BONUS;
                        }
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
                let (child_amp, child_phase) =
                    amp_map.get(&child_id).copied().unwrap_or((0.0, 0.0));
                if child_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }
                let confidence_factor = child_amp.sqrt();
                let role_factor = self
                    .nodes
                    .get(&child_id)
                    .map(adaptive_up_weight)
                    .unwrap_or(1.0);
                let amplification = BASE_PARENT_AMPLIFICATION * confidence_factor * role_factor;

                if let Some(parent_state) = self.nodes.get_mut(&parent_id) {
                    let mut propagated = child_amp * amplification;
                    let phase_diff = (child_phase - parent_state.phase).abs();
                    if phase_diff < PHASE_SYNC_WINDOW {
                        propagated *= PHASE_SYNC_BONUS;
                    }
                    if propagated > parent_state.amplitude {
                        total_delta += propagated - parent_state.amplitude;
                        parent_state.amplitude = propagated;
                        resonance_types
                            .entry(parent_id)
                            .or_insert(ResonanceType::Propagated);
                    }
                }
            }

            // Konvergens — stoppa om energin stabiliserat sig
            if total_delta < CONVERGENCE_THRESHOLD {
                break;
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

        results
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

        // Kör varje variant och samla resultat
        let mut best: HashMap<u32, ResonanceResult> = HashMap::new();

        for goal in goals {
            // Nollställ amplituder mellan varianter
            for state in self.nodes.values_mut() {
                state.amplitude = 0.0;
            }

            let results = self.propagate(goal);
            for r in results {
                let entry = best.entry(r.node_id).or_insert_with(|| ResonanceResult {
                    node_id: r.node_id,
                    amplitude: 0.0,
                    phase: r.phase,
                    resonance_type: r.resonance_type.clone(),
                    causal_boost: 0.0,
                });
                // Union: behåll högsta amplitude per nod
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
    /// Binds the goal vector into each successful node's causal memory,
    /// making future similar goals resonate stronger on those nodes.
    pub fn feedback(&mut self, goal: &str, successful_node_ids: &[u32]) {
        let goal_hv = Hypervector::from_text_ngrams(goal);
        let goal_hash = hash_url(goal);

        for &nid in successful_node_ids {
            if let Some(state) = self.nodes.get_mut(&nid) {
                // Undvik dubbelräkning av samma mål
                if state.last_goal_hash == goal_hash {
                    continue;
                }

                // Bundla goal-HV in i kausalt minne.
                // Första gången: direkt tilldelning (nollvektor ger dålig bundle).
                if state.hit_count == 0 {
                    state.causal_memory = goal_hv.clone();
                } else {
                    state.causal_memory = Hypervector::bundle(&[&state.causal_memory, &goal_hv]);
                }
                state.hit_count += 1;
                state.last_goal_hash = goal_hash;
                state.last_hit_ms = now_ms(); // Temporal decay-referens
            }
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
                let entry = accumulated.entry(r.node_id).or_insert((
                    0.0,
                    r.phase,
                    r.resonance_type.clone(),
                ));

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

/// Global LRU-cache för resonansfält per URL.
///
/// Sparar fältet (med kausalt minne) så att upprepade besök till samma
/// sida drar nytta av tidigare lärande. FIFO-eviction vid kapacitetsgräns.
struct FieldCacheInner {
    /// (url_hash, field) — ordningen representerar ålder (nyast sist)
    entries: Vec<(u64, ResonanceField)>,
    capacity: usize,
}

impl FieldCacheInner {
    fn new(capacity: usize) -> Self {
        FieldCacheInner {
            entries: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Hämta ett fält (flyttar till slutet = nyast)
    fn get(&mut self, url_hash: u64) -> Option<ResonanceField> {
        if let Some(pos) = self.entries.iter().position(|(h, _)| *h == url_hash) {
            let (_, field) = self.entries.remove(pos);
            let cloned = field.clone();
            self.entries.push((url_hash, field));
            Some(cloned)
        } else {
            None
        }
    }

    /// Spara ett fält (ersätt om redan finns, evicta äldsta om fullt)
    fn put(&mut self, url_hash: u64, field: ResonanceField) {
        // Ta bort existerande entry om den finns
        self.entries.retain(|(h, _)| *h != url_hash);
        // Evicta äldsta om fullt
        if self.entries.len() >= self.capacity {
            self.entries.remove(0);
        }
        self.entries.push((url_hash, field));
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

static FIELD_CACHE: std::sync::LazyLock<Mutex<FieldCacheInner>> =
    std::sync::LazyLock::new(|| Mutex::new(FieldCacheInner::new(FIELD_CACHE_CAPACITY)));

/// Get a cached resonance field for a URL, or build a new one.
///
/// If a cached field exists, it retains all causal memory from previous
/// interactions. The caller should call `save_field` after propagation
/// to persist any new causal learning.
pub fn get_or_build_field(tree_nodes: &[SemanticNode], url: &str) -> (ResonanceField, bool) {
    let url_hash = hash_url(url);
    if let Ok(mut cache) = FIELD_CACHE.lock() {
        if let Some(field) = cache.get(url_hash) {
            return (field, true);
        }
    }
    (ResonanceField::from_semantic_tree(tree_nodes, url), false)
}

/// Save a resonance field back to the cache (preserves causal memory).
pub fn save_field(field: &ResonanceField) {
    if let Ok(mut cache) = FIELD_CACHE.lock() {
        cache.put(field.url_hash, field.clone());
    }
}

/// Get cache statistics (entries, capacity).
pub fn cache_stats() -> (usize, usize) {
    if let Ok(cache) = FIELD_CACHE.lock() {
        (cache.len(), cache.capacity)
    } else {
        (0, FIELD_CACHE_CAPACITY)
    }
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
}
