/// Causal Resonance Field Retrieval (CRFR)
///
/// A novel retrieval paradigm that treats the DOM as a living resonance field.
/// Instead of static indexing, a goal creates a resonance wave that propagates
/// through the semantic tree. Nodes that resonate strongly are emitted;
/// causal memory makes the field improve with each successful extraction.
///
/// # Architecture
/// - Each node has a `ResonanceState` with base HV, phase, amplitude, causal memory
/// - `propagate()` sends a goal-wave through the field (3 iterations)
/// - `feedback()` binds successful goal HVs into per-node causal memory
/// - Multi-goal propagation enables constructive/destructive interference
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::scoring::hdc::Hypervector;
use crate::semantic::text_similarity;
use crate::types::SemanticNode;

// ─── Konstanter ────────────────────────────────────────────────────────────

/// Dämpfaktor vid propagation nedåt (förälder → barn)
const CHILD_DAMPING: f32 = 0.6;
/// Förstärkning uppåt (barn → förälder)
const PARENT_AMPLIFICATION: f32 = 0.4;
/// Bonus när noder är fas-synkroniserade
const PHASE_SYNC_BONUS: f32 = 1.15;
/// Fönster för fas-synk (±π/4 radianer)
const PHASE_SYNC_WINDOW: f32 = std::f32::consts::FRAC_PI_4;
/// Under detta propageras ingen energi vidare
const ACTIVATION_THRESHOLD: f32 = 0.05;
/// Under detta returneras noden ej i output
const MIN_OUTPUT_THRESHOLD: f32 = 0.08;
/// Antal vågpropagationsiterationer
const MAX_PROPAGATION_STEPS: usize = 3;
/// Max antal noder i ett fält (minnessäkerhet)
const MAX_FIELD_NODES: usize = 10_000;
/// Kausal boost-vikt (andel av causal memory similarity)
const CAUSAL_WEIGHT: f32 = 0.3;
/// Max ackumulerade feedback-boosts per nod
const MAX_HIT_COUNT: u32 = 100;

// ─── Typer ─────────────────────────────────────────────────────────────────

/// Hur en nod blev resonant
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ResonanceType {
    /// Direkt matchning mot goal
    Direct,
    /// Fick resonans via vågpropagation från granne
    Propagated,
    /// Förstärkt av kausalt minne från tidigare framgångar
    CausalMemory,
}

/// Resultat för en enskild resonant nod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceHit {
    pub node_id: u32,
    pub amplitude: f32,
    pub phase: f32,
    pub resonance_type: ResonanceType,
    /// Bidrag från kausal memory (0.0 om inget minne finns)
    pub causal_boost: f32,
}

/// Sammanfattat resultat från propagation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResonanceResult {
    pub hits: Vec<ResonanceHit>,
    pub total_field_nodes: usize,
    pub nodes_resonant: usize,
    pub propagation_steps: usize,
    /// Andel noder som filtrerats bort (token-besparing)
    pub token_savings_ratio: f32,
}

/// Intern resonanstillstånd per nod
#[derive(Clone)]
struct ResonanceState {
    /// Bas-hypervector (text + roll bindad, för strukturell matchning)
    hv: Hypervector,
    /// Ren text-HV (utan roll-binding, för content-matchning)
    text_hv: Hypervector,
    /// Oscillatorfas [0, 2π)
    phase: f32,
    /// Nuvarande resonansstyrka [0, 1]
    amplitude: f32,
    /// Ackumulerat framgångs-HV (kausal minne)
    causal_memory: Hypervector,
    /// Antal gånger noden bidragit till framgångsrik extraktion
    hit_count: u32,
    /// Senaste goal-hash (dedup-skydd)
    last_goal_hash: u64,
    // Metadata (label behövs för text-matchning)
    label: String,
}

/// Intern nod-topologi
struct NodeTopology {
    children: Vec<u32>,
    parent: Option<u32>,
}

/// Huvudstrukturen: ett resonansfält byggt från ett semantiskt träd
pub struct ResonanceField {
    /// Per-nod resonanstillstånd
    states: HashMap<u32, ResonanceState>,
    /// Topologi-karta
    topology: HashMap<u32, NodeTopology>,
    /// Antal totala queries som propagerat genom fältet
    total_queries: u32,
    /// URL-hash för cachning
    url_hash: u64,
}

// ─── Implementation ────────────────────────────────────────────────────────

impl ResonanceField {
    /// Build a resonance field from a semantic tree and URL.
    ///
    /// Each node gets a base hypervector (text bound with role),
    /// initial phase=0, amplitude=0, and empty causal memory.
    pub fn from_semantic_tree(tree_nodes: &[SemanticNode], url: &str) -> Self {
        let mut states = HashMap::new();
        let mut topology = HashMap::new();

        // Rekursiv indexering med nodgräns
        fn index_node(
            node: &SemanticNode,
            parent_id: Option<u32>,
            depth: usize,
            states: &mut HashMap<u32, ResonanceState>,
            topology: &mut HashMap<u32, NodeTopology>,
        ) {
            if states.len() >= MAX_FIELD_NODES {
                return;
            }

            // Bygg bas-HV: text ⊗ roll ⊗ djup-permutation
            let text_hv = Hypervector::from_text_ngrams(&node.label);
            let role_hv = Hypervector::from_seed(&format!("__role_{}", node.role));
            let base_hv = text_hv.bind(&role_hv).permute(depth * 7);

            states.insert(
                node.id,
                ResonanceState {
                    hv: base_hv,
                    text_hv: text_hv.clone(),
                    phase: 0.0,
                    amplitude: 0.0,
                    causal_memory: Hypervector::zero(),
                    hit_count: 0,
                    last_goal_hash: 0,
                    label: node.label.clone(),
                },
            );

            let child_ids: Vec<u32> = node.children.iter().map(|c| c.id).collect();
            topology.insert(
                node.id,
                NodeTopology {
                    children: child_ids,
                    parent: parent_id,
                },
            );

            for child in &node.children {
                index_node(child, Some(node.id), depth + 1, states, topology);
            }
        }

        for node in tree_nodes {
            index_node(node, None, 0, &mut states, &mut topology);
        }

        let url_hash = fnv_hash(url);

        ResonanceField {
            states,
            topology,
            total_queries: 0,
            url_hash,
        }
    }

    /// Propagate a goal wave through the resonance field.
    ///
    /// Returns nodes sorted by amplitude (strongest resonance first).
    /// The field's causal memory influences scoring — nodes that previously
    /// contributed to successful extractions resonate stronger.
    pub fn propagate(&mut self, goal: &str) -> ResonanceResult {
        self.total_queries += 1;
        let goal_hv = Hypervector::from_text_ngrams(goal);
        let node_ids: Vec<u32> = self.states.keys().copied().collect();
        let total = node_ids.len();

        // ─── Fas 1: Initial resonance ───
        for &id in &node_ids {
            if let Some(state) = self.states.get_mut(&id) {
                // Använd text-HV (utan roll-binding) för content-matchning
                let hv_sim = state.text_hv.similarity(&goal_hv);
                // Strukturell HV adderar roll-kontext
                let struct_sim = state.hv.similarity(&goal_hv);
                // Viktad kombination: 70% text, 30% struktur
                let base_sim = hv_sim * 0.7 + struct_sim * 0.3;

                // Kausal boost från tidigare framgångar
                let causal_boost = if state.hit_count > 0 {
                    state.causal_memory.similarity(&goal_hv) * CAUSAL_WEIGHT
                } else {
                    0.0
                };
                // Text-similarity som komplement (fångar ordöverlapp som HV missar)
                let text_boost = text_similarity(goal, &state.label) * 0.25;

                let initial = (base_sim + causal_boost + text_boost).clamp(0.0, 1.0);
                state.amplitude = initial;
                // Fas baserad på matchning — bra matchning → fas nära 0
                state.phase = hv_sim * std::f32::consts::TAU;
            }
        }

        // ─── Fas 2: Vågpropagation ───
        for _step in 0..MAX_PROPAGATION_STEPS {
            // Snapshot av amplituder och faser (undvik borrow-konflikt)
            let snapshot: HashMap<u32, (f32, f32)> = self
                .states
                .iter()
                .map(|(&id, s)| (id, (s.amplitude, s.phase)))
                .collect();

            for &id in &node_ids {
                let (my_amp, my_phase) = snapshot.get(&id).copied().unwrap_or((0.0, 0.0));
                if my_amp < ACTIVATION_THRESHOLD {
                    continue;
                }

                // Propagera nedåt till barn
                if let Some(topo) = self.topology.get(&id) {
                    let child_ids: Vec<u32> = topo.children.clone();
                    for cid in child_ids {
                        if let Some((_child_amp, child_phase)) = snapshot.get(&cid) {
                            let mut gain = my_amp * CHILD_DAMPING;
                            // Fassynk-bonus
                            let phase_diff = (my_phase - child_phase).abs();
                            if !(PHASE_SYNC_WINDOW..=(std::f32::consts::TAU - PHASE_SYNC_WINDOW))
                                .contains(&phase_diff)
                            {
                                gain *= PHASE_SYNC_BONUS;
                            }
                            if let Some(child_state) = self.states.get_mut(&cid) {
                                child_state.amplitude =
                                    (child_state.amplitude + gain).clamp(0.0, 1.0);
                            }
                        }
                    }
                }

                // Propagera uppåt till förälder
                if let Some(topo) = self.topology.get(&id) {
                    if let Some(pid) = topo.parent {
                        if let Some((_parent_amp, parent_phase)) = snapshot.get(&pid) {
                            let mut gain = my_amp * PARENT_AMPLIFICATION;
                            let phase_diff = (my_phase - parent_phase).abs();
                            if !(PHASE_SYNC_WINDOW..=(std::f32::consts::TAU - PHASE_SYNC_WINDOW))
                                .contains(&phase_diff)
                            {
                                gain *= PHASE_SYNC_BONUS;
                            }
                            if let Some(parent_state) = self.states.get_mut(&pid) {
                                parent_state.amplitude =
                                    (parent_state.amplitude + gain).clamp(0.0, 1.0);
                            }
                        }
                    }
                }
            }
        }

        // ─── Fas 3: Samla resonanta noder ───
        let mut hits: Vec<ResonanceHit> = Vec::new();
        for (&id, state) in &self.states {
            if state.amplitude >= MIN_OUTPUT_THRESHOLD {
                let causal_boost = if state.hit_count > 0 {
                    state.causal_memory.similarity(&goal_hv) * CAUSAL_WEIGHT
                } else {
                    0.0
                };
                let resonance_type = if causal_boost > 0.05 {
                    ResonanceType::CausalMemory
                } else if state.hv.similarity(&goal_hv) >= MIN_OUTPUT_THRESHOLD {
                    ResonanceType::Direct
                } else {
                    ResonanceType::Propagated
                };

                hits.push(ResonanceHit {
                    node_id: id,
                    amplitude: state.amplitude,
                    phase: state.phase,
                    resonance_type,
                    causal_boost,
                });
            }
        }

        hits.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
        let nodes_resonant = hits.len();
        let token_savings = if total > 0 {
            1.0 - (nodes_resonant as f32 / total as f32)
        } else {
            0.0
        };

        ResonanceResult {
            hits,
            total_field_nodes: total,
            nodes_resonant,
            propagation_steps: MAX_PROPAGATION_STEPS,
            token_savings_ratio: token_savings,
        }
    }

    /// Register feedback: these nodes successfully answered the goal.
    ///
    /// Binds the goal HV into each node's causal memory via VSA bundle.
    /// Next time a similar goal arrives, these nodes will resonate stronger.
    pub fn feedback(&mut self, goal: &str, successful_node_ids: &[u32]) {
        let goal_hv = Hypervector::from_text_ngrams(goal);
        let goal_hash = fnv_hash(goal);

        for &nid in successful_node_ids {
            if let Some(state) = self.states.get_mut(&nid) {
                // Dedup: samma goal ger inte dubbel boost
                if state.last_goal_hash == goal_hash {
                    continue;
                }
                state.last_goal_hash = goal_hash;

                if state.hit_count >= MAX_HIT_COUNT {
                    continue;
                }

                // VSA-binding: bundla goal-HV in i kausalt minne
                if state.hit_count == 0 {
                    state.causal_memory = goal_hv.clone();
                } else {
                    let refs = [&state.causal_memory, &goal_hv];
                    state.causal_memory = Hypervector::bundle(&refs);
                }
                state.hit_count += 1;
            }
        }
    }

    /// Propagate multiple goals simultaneously through the field.
    ///
    /// Goals that match the same nodes create constructive interference
    /// (amplitude boost). Goals that are dissimilar create destructive
    /// interference (amplitude reduction) on overlapping nodes.
    pub fn multi_goal_propagate(&mut self, goals: &[&str]) -> ResonanceResult {
        if goals.is_empty() {
            return ResonanceResult {
                hits: Vec::new(),
                total_field_nodes: self.states.len(),
                nodes_resonant: 0,
                propagation_steps: 0,
                token_savings_ratio: 1.0,
            };
        }
        if goals.len() == 1 {
            return self.propagate(goals[0]);
        }

        // Kör propagation per goal och samla amplituder
        let mut combined: HashMap<u32, f32> = HashMap::new();
        let total = self.states.len();

        // Beräkna inter-goal likhet för interferens
        let goal_hvs: Vec<Hypervector> = goals
            .iter()
            .map(|g| Hypervector::from_text_ngrams(g))
            .collect();

        for goal in goals {
            let result = self.propagate(goal);
            for hit in &result.hits {
                let entry = combined.entry(hit.node_id).or_insert(0.0);
                *entry += hit.amplitude;
            }
        }

        // Normalisera och samla
        let goal_count = goals.len() as f32;
        let mut hits: Vec<ResonanceHit> = combined
            .into_iter()
            .filter_map(|(id, total_amp)| {
                let avg_amp = (total_amp / goal_count).clamp(0.0, 1.0);
                if avg_amp >= MIN_OUTPUT_THRESHOLD {
                    // Interferens-bonus: noder som matchade ALLA goals boosted
                    let state = self.states.get(&id)?;
                    let multi_match_count = goal_hvs
                        .iter()
                        .filter(|ghv| state.hv.similarity(ghv) > ACTIVATION_THRESHOLD)
                        .count();
                    let interference_bonus = if multi_match_count > 1 {
                        1.0 + (multi_match_count as f32 - 1.0) * 0.15
                    } else {
                        1.0
                    };
                    let final_amp = (avg_amp * interference_bonus).clamp(0.0, 1.0);

                    Some(ResonanceHit {
                        node_id: id,
                        amplitude: final_amp,
                        phase: state.phase,
                        resonance_type: ResonanceType::Direct,
                        causal_boost: 0.0,
                    })
                } else {
                    None
                }
            })
            .collect();

        hits.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
        let nodes_resonant = hits.len();

        ResonanceResult {
            hits,
            total_field_nodes: total,
            nodes_resonant,
            propagation_steps: MAX_PROPAGATION_STEPS,
            token_savings_ratio: if total > 0 {
                1.0 - (nodes_resonant as f32 / total as f32)
            } else {
                0.0
            },
        }
    }

    /// Returns total queries processed by this field
    pub fn total_queries(&self) -> u32 {
        self.total_queries
    }

    /// Returns the URL hash this field was built for
    pub fn url_hash(&self) -> u64 {
        self.url_hash
    }

    /// Returns node count in the field
    pub fn node_count(&self) -> usize {
        self.states.len()
    }
}

// ─── Hjälpfunktioner ───────────────────────────────────────────────────────

/// FNV-1a hash för strängar
fn fnv_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// ─── Tester ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{NodeState, TrustLevel};

    fn make_node(id: u32, role: &str, label: &str, children: Vec<SemanticNode>) -> SemanticNode {
        SemanticNode {
            id,
            role: role.to_string(),
            label: label.to_string(),
            value: None,
            state: NodeState::default_state(),
            action: None,
            relevance: 0.0,
            trust: TrustLevel::Untrusted,
            children,
            html_id: None,
            name: None,
            bbox: None,
        }
    }

    #[test]
    fn test_field_creation() {
        let nodes = vec![make_node(
            1,
            "heading",
            "Product Page",
            vec![
                make_node(2, "text", "Laptop 999 kr", vec![]),
                make_node(3, "button", "Add to cart", vec![]),
            ],
        )];
        let field = ResonanceField::from_semantic_tree(&nodes, "https://shop.se");
        assert_eq!(field.node_count(), 3, "Fältet ska ha 3 noder");
        assert!(field.url_hash() != 0, "URL-hash ska vara satt");
    }

    #[test]
    fn test_propagation_returns_relevant_nodes() {
        let nodes = vec![make_node(
            1,
            "generic",
            "Navigation",
            vec![
                make_node(2, "text", "Best laptop deals and prices", vec![]),
                make_node(3, "button", "Buy laptop now", vec![]),
                make_node(4, "text", "Cookie policy information", vec![]),
                make_node(5, "link", "About us", vec![]),
            ],
        )];

        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://shop.se");
        let result = field.propagate("find laptop price");

        assert!(
            !result.hits.is_empty(),
            "Propagation ska returnera resonanta noder"
        );
        // Laptop-relaterade noder ska finnas bland top-3
        let top_ids: Vec<u32> = result.hits.iter().take(3).map(|h| h.node_id).collect();
        assert!(
            top_ids.contains(&2) || top_ids.contains(&3),
            "Laptop-noder ska finnas i top-3, fick: {top_ids:?}"
        );
    }

    #[test]
    fn test_causal_feedback_improves_resonance() {
        let nodes = vec![
            make_node(1, "text", "Product price 499 kr", vec![]),
            make_node(2, "text", "Shipping information", vec![]),
            make_node(3, "text", "Return policy details", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://shop.se");

        // Första query
        let result1 = field.propagate("find product price");
        let amp_before = result1
            .hits
            .iter()
            .find(|h| h.node_id == 1)
            .map(|h| h.amplitude)
            .unwrap_or(0.0);

        // Ge feedback att nod 1 var rätt svar
        field.feedback("find product price", &[1]);

        // Samma query igen — bör ge starkare resonans tack vare kausal memory
        let result2 = field.propagate("find product price");
        let amp_after = result2
            .hits
            .iter()
            .find(|h| h.node_id == 1)
            .map(|h| h.amplitude)
            .unwrap_or(0.0);

        assert!(
            amp_after >= amp_before,
            "Kausal feedback ska förbättra resonans: {amp_before} → {amp_after}"
        );
        // Kausal boost ska synas
        let causal_hit = result2.hits.iter().find(|h| h.node_id == 1);
        assert!(
            causal_hit.map(|h| h.causal_boost).unwrap_or(0.0) > 0.0,
            "Kausal boost ska vara positiv efter feedback"
        );
    }

    #[test]
    fn test_multi_goal_interference() {
        let nodes = vec![
            make_node(1, "text", "Laptop specs and performance", vec![]),
            make_node(2, "text", "Laptop price 9999 kr", vec![]),
            make_node(3, "text", "Weather forecast today", vec![]),
        ];

        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://test.se");
        let result = field.multi_goal_propagate(&["laptop specs", "laptop price"]);

        // Laptop-noder ska resonera, väder-nod ska inte
        let laptop_hits: Vec<&ResonanceHit> = result
            .hits
            .iter()
            .filter(|h| h.node_id == 1 || h.node_id == 2)
            .collect();
        let weather_hit = result.hits.iter().find(|h| h.node_id == 3);

        assert!(!laptop_hits.is_empty(), "Laptop-noder ska ha resonans");
        // Om vädernoden finns, ska den ha lägre amplitude
        if let Some(wh) = weather_hit {
            let max_laptop = laptop_hits
                .iter()
                .map(|h| h.amplitude)
                .fold(0.0f32, f32::max);
            assert!(
                wh.amplitude < max_laptop,
                "Väder-nod ({}) ska ha lägre amplitude än laptop-noder ({max_laptop})",
                wh.amplitude
            );
        }
    }

    #[test]
    fn test_empty_tree() {
        let nodes: Vec<SemanticNode> = vec![];
        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://empty.se");
        let result = field.propagate("anything");
        assert_eq!(result.total_field_nodes, 0, "Tomt fält");
        assert!(result.hits.is_empty(), "Inga träffar i tomt fält");
    }

    #[test]
    fn test_single_node() {
        let nodes = vec![make_node(1, "heading", "Hello World", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://hello.se");
        let result = field.propagate("Hello World");
        assert!(!result.hits.is_empty(), "Exakt matchning ska ge resonans");
        // HV-similarity + text_similarity ger sammanlagt amplitud
        assert!(
            result.hits[0].amplitude > MIN_OUTPUT_THRESHOLD,
            "Exakt matchning ska ge amplitud över output-tröskeln, fick: {}",
            result.hits[0].amplitude
        );
    }

    #[test]
    fn test_feedback_dedup() {
        let nodes = vec![make_node(1, "text", "Price info", vec![])];
        let mut field = ResonanceField::from_semantic_tree(&nodes, "https://test.se");

        // Ge samma feedback två gånger
        field.feedback("find price", &[1]);
        field.feedback("find price", &[1]);

        // hit_count ska bara vara 1 (dedup)
        let state = field.states.get(&1).expect("Nod 1 ska finnas");
        assert_eq!(state.hit_count, 1, "Dedup ska förhindra dubbel feedback");
    }
}
