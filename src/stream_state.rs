// Fas 16: Goal-Driven Adaptive DOM Streaming – State Manager + Decision Layer
//
// Ren sync-modul utan I/O. Håller all mutable state för en stream-session
// och beslutslager för goal-baserad nod-routing.
//
// StreamState: session-state (sent_nodes, directives, counters)
// DecisionLayer: goal-scoring och routing av noder
// Directive: LLM-kommandon (expand, stop, next_branch, lower_threshold)

use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::semantic::text_similarity;
use crate::types::SemanticNode;

// ─── Directives (LLM → server) ──────────────────────────────────────────────

/// Directive från LLM:en under streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Directive {
    /// Expandera en specifik nod – traversera och emittera dess barn
    Expand { node_id: u32 },
    /// Stäng streamen omedelbart
    Stop,
    /// Hoppa till nästa topprankade gren
    NextBranch,
    /// Sänk min_relevance dynamiskt
    LowerThreshold { value: f32 },
}

// ─── State Manager ───────────────────────────────────────────────────────────

/// Ren sync-struct för session-state. Ingen async, inga futures, ingen I/O.
pub struct StreamState {
    /// Vilka nod-ID:n som redan skickats – O(1) lookup, aldrig dubbel-emit
    pub sent_nodes: HashSet<u32>,

    /// Vilka grenar som är fullt expanderade
    pub expanded_nodes: HashSet<u32>,

    /// Inkommande directives från LLM
    pub directive_queue: VecDeque<Directive>,

    /// Antal noder emitterade totalt
    pub nodes_emitted: usize,

    /// Antal noder sedda (traverserade) totalt
    pub nodes_seen: usize,

    /// Justerbar runtime via lower_threshold-directive
    pub relevance_threshold: f32,

    /// Hård gräns – stream stängs oavsett om LLM inte skickat stop
    pub max_nodes: usize,

    /// Totalt antal noder i DOM:en (sätts vid parsning)
    pub total_dom_nodes: usize,

    /// Nästa chunk-id
    pub next_chunk_id: u32,

    /// Högsta relevans sedd hittills
    pub top_relevance: f32,

    /// Om streamen är stoppad
    stopped: bool,
}

impl StreamState {
    /// Skapa nytt state med konfigurerade gränser
    pub fn new(relevance_threshold: f32, max_nodes: usize) -> Self {
        StreamState {
            sent_nodes: HashSet::new(),
            expanded_nodes: HashSet::new(),
            directive_queue: VecDeque::new(),
            nodes_emitted: 0,
            nodes_seen: 0,
            relevance_threshold,
            max_nodes,
            total_dom_nodes: 0,
            next_chunk_id: 0,
            top_relevance: 0.0,
            stopped: false,
        }
    }

    /// Markera nod som skickad – returnerar false om redan skickad
    pub fn mark_sent(&mut self, id: u32) -> bool {
        if self.sent_nodes.insert(id) {
            self.nodes_emitted += 1;
            true
        } else {
            false
        }
    }

    /// Markera nod som expanderad
    pub fn mark_expanded(&mut self, id: u32) {
        self.expanded_nodes.insert(id);
    }

    /// Är nod redan expanderad?
    pub fn is_expanded(&self, id: u32) -> bool {
        self.expanded_nodes.contains(&id)
    }

    /// Lägg till directive i kön
    pub fn push_directive(&mut self, d: Directive) {
        self.directive_queue.push_back(d);
    }

    /// Ta nästa directive från kön
    pub fn next_directive(&mut self) -> Option<Directive> {
        self.directive_queue.pop_front()
    }

    /// Ska en nod med given relevans emittas?
    pub fn should_emit(&self, relevance: f32) -> bool {
        relevance >= self.relevance_threshold
    }

    /// Uppdatera top_relevance om ny nod har högre score
    pub fn update_top_relevance(&mut self, relevance: f32) {
        if relevance > self.top_relevance {
            self.top_relevance = relevance;
        }
    }

    /// Registrera att en nod traverserats (oavsett om den emitteras)
    pub fn mark_seen(&mut self, count: usize) {
        self.nodes_seen += count;
    }

    /// Är streamen färdig? (max_nodes nådd eller explicit stop)
    pub fn is_done(&self) -> bool {
        self.stopped || self.nodes_emitted >= self.max_nodes
    }

    /// Markera streamen som stoppad
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Nästa chunk-id (auto-increment)
    pub fn next_chunk(&mut self) -> u32 {
        let id = self.next_chunk_id;
        self.next_chunk_id += 1;
        id
    }
}

// ─── Node Decision ───────────────────────────────────────────────────────────

/// Beslut för en enskild nod
#[derive(Debug, Clone, PartialEq)]
pub enum NodeDecision {
    /// Emittera noden direkt
    Emit,
    /// Pruna – skippa helt
    Prune,
    /// Köa för eventuell expansion (sparar prioritet)
    Queue { priority: f32 },
}

// ─── Decision Layer ──────────────────────────────────────────────────────────

/// Goal-scoring och routing av noder. Ren logik, ingen I/O.
pub struct DecisionLayer {
    /// LLM:ens goal
    goal: String,
    /// Goal i lowercase (cachad)
    goal_lower: String,
}

impl DecisionLayer {
    pub fn new(goal: &str) -> Self {
        DecisionLayer {
            goal: goal.to_string(),
            goal_lower: goal.to_lowercase(),
        }
    }

    /// Beräkna relevance-score per nod baserat på goal
    pub fn score(&self, node: &SemanticNode) -> f32 {
        let text_score = text_similarity(&self.goal_lower, &node.label);
        let role_score = SemanticNode::role_priority(&node.role);

        // Viktat: text 50%, roll 40%, bas 10%
        let raw = (text_score * 0.5) + (role_score * 0.4) + 0.05;
        raw.clamp(0.0, 1.0)
    }

    /// Rutta en nod: emit, prune, eller queue
    pub fn route(&self, node: &SemanticNode, state: &StreamState) -> NodeDecision {
        // Redan skickad?
        if state.sent_nodes.contains(&node.id) {
            return NodeDecision::Prune;
        }

        let score = self.score(node);

        // Interaktiva element och headings passerar alltid threshold
        let is_interactive = node.action.is_some() || node.role == "heading" || node.role == "link";

        if state.should_emit(score) || is_interactive {
            NodeDecision::Emit
        } else if score > 0.1 {
            NodeDecision::Queue { priority: score }
        } else {
            NodeDecision::Prune
        }
    }

    /// Getter för goal
    pub fn goal(&self) -> &str {
        &self.goal
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_manager_no_double_emit() {
        let mut state = StreamState::new(0.3, 50);
        assert!(state.mark_sent(1), "Första emit borde lyckas");
        assert!(!state.mark_sent(1), "Dubbel-emit borde blockeras");
        assert_eq!(state.nodes_emitted, 1, "Räknaren borde bara vara 1");
    }

    #[test]
    fn state_manager_expand_marks_node() {
        let mut state = StreamState::new(0.3, 50);
        assert!(
            !state.is_expanded(42),
            "Nod borde inte vara expanderad initialt"
        );
        state.mark_expanded(42);
        assert!(
            state.is_expanded(42),
            "Nod borde vara expanderad efter mark"
        );
    }

    #[test]
    fn state_manager_directive_queue_fifo() {
        let mut state = StreamState::new(0.3, 50);
        state.push_directive(Directive::Expand { node_id: 10 });
        state.push_directive(Directive::Stop);

        match state.next_directive() {
            Some(Directive::Expand { node_id: 10 }) => {}
            other => panic!("Förväntat Expand(10), fick {:?}", other),
        }
        match state.next_directive() {
            Some(Directive::Stop) => {}
            other => panic!("Förväntat Stop, fick {:?}", other),
        }
        assert!(state.next_directive().is_none(), "Kön borde vara tom");
    }

    #[test]
    fn state_manager_is_done_max_nodes() {
        let mut state = StreamState::new(0.0, 3);
        assert!(!state.is_done(), "Borde inte vara klar initialt");
        state.mark_sent(1);
        state.mark_sent(2);
        state.mark_sent(3);
        assert!(
            state.is_done(),
            "Borde vara klar efter max_nodes emitterade"
        );
    }

    #[test]
    fn state_manager_is_done_stop() {
        let mut state = StreamState::new(0.3, 50);
        assert!(!state.is_done(), "Borde inte vara klar initialt");
        state.stop();
        assert!(state.is_done(), "Borde vara klar efter stop");
    }

    #[test]
    fn state_manager_lower_threshold_directive() {
        let mut state = StreamState::new(0.5, 50);
        assert!(
            !state.should_emit(0.3),
            "0.3 borde inte emittas med threshold 0.5"
        );
        state.relevance_threshold = 0.2;
        assert!(
            state.should_emit(0.3),
            "0.3 borde emittas med threshold 0.2"
        );
    }

    #[test]
    fn decision_layer_prunes_low_relevance() {
        let decision = DecisionLayer::new("köp biljett");
        let node = SemanticNode::new(1, "text", "Cookie-inställningar för webbplatsen");
        let state = StreamState::new(0.5, 50);

        let decision_result = decision.route(&node, &state);
        // Låg relevans text borde antingen prunas eller köas – inte emittas
        assert_ne!(
            decision_result,
            NodeDecision::Emit,
            "Irrelevant nod borde inte emittas direkt"
        );
    }

    #[test]
    fn decision_layer_emits_relevant_node() {
        let decision = DecisionLayer::new("köp biljett");
        let mut node = SemanticNode::new(1, "cta", "Köp biljett nu");
        node.action = Some("click".to_string());
        let state = StreamState::new(0.3, 50);

        let result = decision.route(&node, &state);
        assert_eq!(result, NodeDecision::Emit, "Relevant CTA borde emittas");
    }

    #[test]
    fn decision_layer_skips_already_sent() {
        let decision = DecisionLayer::new("test");
        let node = SemanticNode::new(5, "button", "Klicka här");
        let mut state = StreamState::new(0.0, 50);
        state.mark_sent(5);

        let result = decision.route(&node, &state);
        assert_eq!(
            result,
            NodeDecision::Prune,
            "Redan skickad nod borde prunas"
        );
    }

    #[test]
    fn decision_layer_score_basic() {
        let decision = DecisionLayer::new("hitta pris");
        let node = SemanticNode::new(1, "price", "Pris: 299 kr");
        let score = decision.score(&node);
        assert!(score > 0.3, "Prisnod borde ha hög score, fick {}", score);
    }
}
