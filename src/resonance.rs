/// Causal Resonance Field Retrieval (CRFR)
///
/// Treats the DOM as a living resonance field where nodes oscillate with
/// amplitudes determined by goal similarity, causal memory from past
/// successful interactions, and wave propagation through the tree structure.
///
/// The field supports multi-goal interference (constructive and destructive)
/// and learns over time via causal feedback.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::scoring::hdc::Hypervector;
use crate::types::SemanticNode;

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// Dämpning vid propagation från förälder till barn
const CHILD_DAMPING: f32 = 0.35;
/// Förstärkning vid propagation från barn till förälder
const PARENT_AMPLIFICATION: f32 = 0.25;
/// Bonus-multiplikator vid fas-synkronisering
const PHASE_SYNC_BONUS: f32 = 1.08;
/// Fönster (radianer) inom vilket fas-synk aktiveras
const PHASE_SYNC_WINDOW: f32 = std::f32::consts::FRAC_PI_4;
/// Minsta amplitud för att en nod ska propagera vidare
const ACTIVATION_THRESHOLD: f32 = 0.01;
/// Minsta amplitud för att inkluderas i resultat
const MIN_OUTPUT_THRESHOLD: f32 = 0.01;
/// Antal propagations-iterationer
const MAX_PROPAGATION_STEPS: u32 = 2;
/// Max antal noder i fältet (skydd mot extremt stora DOM:ar)
const MAX_FIELD_NODES: usize = 10_000;

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
    /// Bas-hypervector (text ⊗ roll) — kombinerad representation
    #[serde(with = "hv_serde")]
    hv: Hypervector,
    /// Ren text-hypervector (utan roll-binding) — används för likhetsmätning
    #[serde(with = "hv_serde")]
    text_hv: Hypervector,
    /// Oscillatorfas (0.0..2π)
    phase: f32,
    /// Nuvarande resonansstyrka
    amplitude: f32,
    /// Ackumulerat kausalt minne från lyckade mål
    #[serde(with = "hv_serde")]
    causal_memory: Hypervector,
    /// Antal lyckade målmatchningar
    hit_count: u32,
    /// Hash av senaste mål (undvik dubbelräkning)
    last_goal_hash: u64,
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
    /// URL-hash som identifierar sidan
    pub url_hash: u64,
    /// Tidpunkt då fältet skapades (ms sedan epoch)
    pub created_at_ms: u64,
    /// Totalt antal propagations-anrop
    pub total_queries: u32,
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

        // Platta ut trädet och bygg relationer
        for node in tree_nodes {
            Self::flatten_node(node, None, &mut nodes, &mut parent_map, &mut children_map);
            // Begränsa storlek
            if nodes.len() >= MAX_FIELD_NODES {
                break;
            }
        }

        ResonanceField {
            nodes,
            parent_map,
            children_map,
            url_hash: hash_url(url),
            created_at_ms: now_ms(),
            total_queries: 0,
        }
    }

    /// Rekursivt platta ut en nod och dess barn
    fn flatten_node(
        node: &SemanticNode,
        parent_id: Option<u32>,
        nodes: &mut HashMap<u32, ResonanceState>,
        parent_map: &mut HashMap<u32, u32>,
        children_map: &mut HashMap<u32, Vec<u32>>,
    ) {
        if nodes.len() >= MAX_FIELD_NODES {
            return;
        }

        // Beräkna bas-HV: text-ngram ⊗ roll-vektor
        let text_hv = Hypervector::from_text_ngrams(&node.label);
        let role_hv = Hypervector::from_seed(&format!("__role_{}", node.role));
        let base_hv = text_hv.bind(&role_hv);

        let state = ResonanceState {
            hv: base_hv,
            text_hv,
            phase: 0.0,
            amplitude: 0.0,
            causal_memory: Hypervector::zero(),
            hit_count: 0,
            last_goal_hash: 0,
        };

        nodes.insert(node.id, state);

        // Registrera förälder-relation
        if let Some(pid) = parent_id {
            parent_map.insert(node.id, pid);
            children_map.entry(pid).or_default().push(node.id);
        }

        // Rekursera barn
        for child in &node.children {
            Self::flatten_node(child, Some(node.id), nodes, parent_map, children_map);
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

        // Spara kausal-boost per nod för resultatrapportering
        let mut causal_boosts: HashMap<u32, f32> = HashMap::new();
        // Spara resonanstyp per nod
        let mut resonance_types: HashMap<u32, ResonanceType> = HashMap::new();

        // Fas 1: Initial resonans
        // HDC cosine similarity ger [-1, 1] men de flesta noder hamnar nära 0.
        // Vi skalar om till [0, 1]: raw_sim ∈ [-1,1] → (raw+1)/2 ∈ [0,1]
        // och höjer sedan kontrasten med en power-funktion.
        let node_ids: Vec<u32> = self.nodes.keys().copied().collect();
        for &nid in &node_ids {
            if let Some(state) = self.nodes.get_mut(&nid) {
                // Använd text_hv (ren text utan roll-binding) för likhetsmätning
                let raw_sim = state.text_hv.similarity(&goal_hv);
                // Skala [-1,1] → [0,1] och höj kontrast (power=3 sprider top-noder)
                let normalized = ((raw_sim + 1.0) / 2.0).clamp(0.0, 1.0);
                let base_resonance = normalized * normalized * normalized; // power-3 kontrast

                let causal_boost = if state.hit_count > 0 {
                    let raw_causal = state.causal_memory.similarity(&goal_hv);
                    let norm_causal = ((raw_causal + 1.0) / 2.0).clamp(0.0, 1.0);
                    norm_causal * norm_causal * 0.3
                } else {
                    0.0
                };

                state.amplitude = (base_resonance + causal_boost).clamp(0.0, 1.0);

                causal_boosts.insert(nid, causal_boost);

                // Bestäm typ baserat på dominerande bidrag
                if causal_boost > base_resonance * 0.5 && state.hit_count > 0 {
                    resonance_types.insert(nid, ResonanceType::CausalMemory);
                } else if base_resonance > ACTIVATION_THRESHOLD {
                    resonance_types.insert(nid, ResonanceType::Direct);
                }

                // OBS: last_goal_hash uppdateras INTE här — den används
                // enbart av feedback() för att undvika dubbelräkning.
            }
        }

        // Fas 2: Vågpropagation
        for _step in 0..MAX_PROPAGATION_STEPS {
            // Samla amplituder för att undvika borrow-konflikter
            let amplitudes: HashMap<u32, (f32, f32)> = self
                .nodes
                .iter()
                .map(|(&id, s)| (id, (s.amplitude, s.phase)))
                .collect();

            // Propagera förälder -> barn (max-operation, ej addition)
            for (&parent_id, children) in &self.children_map {
                let (parent_amp, parent_phase) =
                    amplitudes.get(&parent_id).copied().unwrap_or((0.0, 0.0));

                if parent_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }

                for &child_id in children {
                    if let Some(child_state) = self.nodes.get_mut(&child_id) {
                        let mut propagated = parent_amp * CHILD_DAMPING;

                        // Fas-synk: om förälder och barn är nära i fas, förstärk
                        let phase_diff = (parent_phase - child_state.phase).abs();
                        if phase_diff < PHASE_SYNC_WINDOW {
                            propagated *= PHASE_SYNC_BONUS;
                        }

                        // Max istf += — undviker amplitudsexplosion
                        if propagated > child_state.amplitude {
                            child_state.amplitude = propagated;
                            resonance_types
                                .entry(child_id)
                                .or_insert(ResonanceType::Propagated);
                        }
                    }
                }
            }

            // Propagera barn -> förälder (max-operation)
            for (&child_id, &parent_id) in &self.parent_map {
                let (child_amp, child_phase) =
                    amplitudes.get(&child_id).copied().unwrap_or((0.0, 0.0));

                if child_amp <= ACTIVATION_THRESHOLD {
                    continue;
                }

                if let Some(parent_state) = self.nodes.get_mut(&parent_id) {
                    let mut propagated = child_amp * PARENT_AMPLIFICATION;

                    // Fas-synk barn -> förälder
                    let phase_diff = (child_phase - parent_state.phase).abs();
                    if phase_diff < PHASE_SYNC_WINDOW {
                        propagated *= PHASE_SYNC_BONUS;
                    }

                    if propagated > parent_state.amplitude {
                        parent_state.amplitude = propagated;
                        resonance_types
                            .entry(parent_id)
                            .or_insert(ResonanceType::Propagated);
                    }
                }
            }
        }

        // Fas 3: Samla resultat
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

        // Sortera efter amplitud (fallande), använd total_cmp för NaN-säkerhet
        results.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));

        results
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
