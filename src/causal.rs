/// Causal Action Graph – Fas 9a
///
/// Modellerar sidtillståndsövergångar som en riktad graf.
/// Agenten resonerar om konsekvenser *innan* den agerar.
///
/// Pipeline:
/// 1. Bygg graf från temporal historik (snapshots + actions)
/// 2. Varje nod = sidtillstånd, varje kant = åtgärd
/// 3. predict_outcome: givet en action, vilka tillstånd kan uppstå?
/// 4. find_safest_path: hitta väg med lägst risk mot målet
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::compiler::ActionType;
use crate::semantic::text_similarity;

// ─── Types ──────────────────────────────────────────────────────────────────

/// Input-format för snapshot vid build_causal_graph (accepterar JSON-objekt)
#[derive(Debug, Clone, Deserialize)]
pub struct CausalSnapshotInput {
    pub url: String,
    #[serde(default)]
    pub node_count: u32,
    #[serde(default)]
    pub warning_count: u32,
    #[serde(default)]
    pub key_elements: Vec<String>,
}

/// Ett tillstånd i den kausala grafen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalState {
    /// Unikt tillstånds-ID
    pub state_id: u32,
    /// URL vid detta tillstånd
    pub url: String,
    /// Antal noder i det semantiska trädet
    pub node_count: u32,
    /// Antal injection-varningar
    pub warning_count: u32,
    /// Nyckel-element (roll:label) som identifierar tillståndet
    pub key_elements: Vec<String>,
    /// Antal gånger tillståndet observerats
    pub visit_count: u32,
}

/// En övergång mellan två tillstånd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    /// Källtillstånd
    pub from_state: u32,
    /// Måltillstånd
    pub to_state: u32,
    /// Vilken action som orsakade övergången
    pub action: String,
    /// Action-typ
    pub action_type: ActionType,
    /// Sannolikhet (0.0–1.0) baserad på historisk frekvens
    pub probability: f32,
    /// Risk-poäng: hur riskabelt är denna övergång?
    pub risk_score: f32,
    /// Antal gånger denna övergång observerats
    pub observation_count: u32,
}

/// Predikterat resultat av en action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedOutcome {
    /// Möjliga resulterande tillstånd med sannolikhet
    pub outcomes: Vec<OutcomeEntry>,
    /// Total risk-poäng (vägt medel)
    pub aggregate_risk: f32,
    /// Rekommendation
    pub recommendation: String,
}

/// En möjlig utgång
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeEntry {
    /// Tillstånds-ID
    pub state_id: u32,
    /// URL
    pub url: String,
    /// Sannolikhet
    pub probability: f32,
    /// Risk
    pub risk_score: f32,
    /// Förväntade varningar
    pub expected_warnings: u32,
}

/// Den kausala grafen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalGraph {
    /// Alla kända tillstånd
    pub states: Vec<CausalState>,
    /// Alla kända övergångar
    pub edges: Vec<CausalEdge>,
    /// Aktuellt tillstånd (om känt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_state_id: Option<u32>,
}

/// Säkraste vägen till mål
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafePath {
    /// Steg i vägen (tillstånds-IDn)
    pub path: Vec<u32>,
    /// Åtgärder mellan stegen
    pub actions: Vec<String>,
    /// Total risk
    pub total_risk: f32,
    /// Total sannolikhet att nå målet
    pub success_probability: f32,
    /// Sammanfattning
    pub summary: String,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl CausalGraph {
    /// Skapa ny tom graf
    pub fn new() -> Self {
        CausalGraph {
            states: vec![],
            edges: vec![],
            current_state_id: None,
        }
    }

    /// Lägg till eller uppdatera ett tillstånd
    pub fn add_state(
        &mut self,
        url: &str,
        node_count: u32,
        warning_count: u32,
        key_elements: Vec<String>,
    ) -> u32 {
        // Matcha mot befintligt tillstånd via URL + nyckel-element
        if let Some(existing) = self
            .states
            .iter_mut()
            .find(|s| s.url == url && s.key_elements == key_elements)
        {
            existing.visit_count += 1;
            return existing.state_id;
        }

        let state_id = self.states.len() as u32;
        self.states.push(CausalState {
            state_id,
            url: url.to_string(),
            node_count,
            warning_count,
            key_elements,
            visit_count: 1,
        });
        state_id
    }

    /// Registrera en övergång mellan tillstånd
    pub fn add_edge(
        &mut self,
        from_state: u32,
        to_state: u32,
        action: &str,
        action_type: ActionType,
    ) {
        // Uppdatera befintlig kant om den finns
        let existing_idx = self.edges.iter().position(|e| {
            e.from_state == from_state && e.to_state == to_state && e.action == action
        });

        if let Some(idx) = existing_idx {
            self.edges[idx].observation_count += 1;
            self.normalize_probabilities(from_state);
            return;
        }

        // Beräkna risk baserat på varningsförändringar
        let from_warnings = self
            .states
            .get(from_state as usize)
            .map(|s| s.warning_count)
            .unwrap_or(0);
        let to_warnings = self
            .states
            .get(to_state as usize)
            .map(|s| s.warning_count)
            .unwrap_or(0);

        let risk_score = if to_warnings > from_warnings {
            // Varningar ökar → högre risk
            ((to_warnings - from_warnings) as f32 * 0.3).min(1.0)
        } else {
            0.0
        };

        self.edges.push(CausalEdge {
            from_state,
            to_state,
            action: action.to_string(),
            action_type,
            probability: 1.0, // Uppdateras vid normalisering
            risk_score,
            observation_count: 1,
        });

        // Normalisera sannolikheter för alla kanter från samma tillstånd
        self.normalize_probabilities(from_state);
    }

    /// Normalisera sannolikheter för utgående kanter
    fn normalize_probabilities(&mut self, from_state: u32) {
        let total: u32 = self
            .edges
            .iter()
            .filter(|e| e.from_state == from_state)
            .map(|e| e.observation_count)
            .sum();

        if total == 0 {
            return;
        }

        for edge in self.edges.iter_mut().filter(|e| e.from_state == from_state) {
            edge.probability = edge.observation_count as f32 / total as f32;
        }
    }

    /// Prediktera resultat av en action från aktuellt tillstånd
    pub fn predict_outcome(&self, action: &str, from_state_id: Option<u32>) -> PredictedOutcome {
        let current = from_state_id.or(self.current_state_id).unwrap_or(0);

        let matching_edges: Vec<&CausalEdge> = self
            .edges
            .iter()
            .filter(|e| e.from_state == current && text_similarity(&e.action, action) > 0.5)
            .collect();

        if matching_edges.is_empty() {
            return PredictedOutcome {
                outcomes: vec![],
                aggregate_risk: 0.5,
                recommendation:
                    "Okänd action – ingen historisk data. Rekommenderar försiktig utforskning."
                        .to_string(),
            };
        }

        let outcomes: Vec<OutcomeEntry> = matching_edges
            .iter()
            .map(|edge| {
                let state = self.states.get(edge.to_state as usize);
                OutcomeEntry {
                    state_id: edge.to_state,
                    url: state.map(|s| s.url.clone()).unwrap_or_default(),
                    probability: edge.probability,
                    risk_score: edge.risk_score,
                    expected_warnings: state.map(|s| s.warning_count).unwrap_or(0),
                }
            })
            .collect();

        let aggregate_risk = if outcomes.is_empty() {
            0.5
        } else {
            outcomes.iter().map(|o| o.risk_score * o.probability).sum()
        };

        let recommendation = if aggregate_risk > 0.7 {
            "HÖG RISK – överväg alternativ väg".to_string()
        } else if aggregate_risk > 0.3 {
            "MEDIUM RISK – fortsätt med försiktighet".to_string()
        } else {
            "LÅG RISK – säker att fortsätta".to_string()
        };

        PredictedOutcome {
            outcomes,
            aggregate_risk,
            recommendation,
        }
    }

    /// Hitta säkraste vägen till ett mål (BFS med risk-viktning)
    ///
    /// Använder flernivå semantisk matchning:
    /// 1. Direkt text_similarity mot URL och nyckel-element
    /// 2. Ordnivå-matchning: varje ord i goal matchas mot varje element
    /// 3. Kontextord-mappning: "kontakt" matchar telefon/email-mönster etc.
    pub fn find_safest_path(&self, goal: &str, max_depth: u32) -> SafePath {
        let start = self.current_state_id.unwrap_or(0);

        // Hitta mål-tillstånd med flernivå semantisk matchning + scoring
        let mut scored_states: Vec<(u32, f32)> = self
            .states
            .iter()
            .filter(|s| semantic_goal_match(goal, s))
            .map(|s| (s.state_id, semantic_goal_score(goal, s)))
            .collect();
        // Sortera efter poäng (högst först)
        scored_states.sort_by(|a, b| b.1.total_cmp(&a.1));

        // Om start-staten matchar men det finns bättre matchande states,
        // exkludera start-staten från goal_states (den är inte målet utan startpunkten)
        let goal_states: Vec<u32> = if scored_states.len() > 1 {
            let best_score = scored_states[0].1;
            let start_score = scored_states
                .iter()
                .find(|(id, _)| *id == start)
                .map(|(_, s)| *s)
                .unwrap_or(0.0);
            if start_score < best_score {
                // Det finns bättre mål — exkludera start
                scored_states
                    .iter()
                    .filter(|(id, _)| *id != start)
                    .map(|(id, _)| *id)
                    .collect()
            } else {
                scored_states.iter().map(|(id, _)| *id).collect()
            }
        } else {
            scored_states.iter().map(|(id, _)| *id).collect()
        };

        if goal_states.is_empty() {
            return SafePath {
                path: vec![start],
                actions: vec![],
                total_risk: 0.0,
                success_probability: 0.0,
                summary: "Inget känt mål-tillstånd hittades i grafen".to_string(),
            };
        }

        // BFS med risk-viktning
        type BfsEntry = (u32, Vec<u32>, Vec<String>, f32, f32);
        let mut queue: Vec<BfsEntry> = vec![(start, vec![start], vec![], 0.0, 1.0)];
        let mut visited: HashMap<u32, f32> = HashMap::new();
        visited.insert(start, 0.0);

        let mut best_path: Option<SafePath> = None;

        while let Some((current, path, actions, risk, prob)) = queue.pop() {
            if path.len() as u32 > max_depth {
                continue;
            }

            // Nådde vi målet?
            if goal_states.contains(&current) {
                let candidate = SafePath {
                    path: path.clone(),
                    actions: actions.clone(),
                    total_risk: risk,
                    success_probability: prob,
                    summary: format!(
                        "{} steg, risk {:.0}%, sannolikhet {:.0}%",
                        actions.len(),
                        risk * 100.0,
                        prob * 100.0
                    ),
                };

                // Behåll bästa (lägst risk × högst sannolikhet)
                let score = risk - prob;
                let best_score = best_path
                    .as_ref()
                    .map(|b| b.total_risk - b.success_probability)
                    .unwrap_or(f32::MAX);

                if score < best_score {
                    best_path = Some(candidate);
                }
                continue;
            }

            // Expandera grannar
            for edge in self.edges.iter().filter(|e| e.from_state == current) {
                let new_risk = risk + edge.risk_score * (1.0 - risk);
                let new_prob = prob * edge.probability;

                // Besök bara om vi hittar en bättre väg
                let prev_risk = visited.get(&edge.to_state).copied().unwrap_or(f32::MAX);
                if new_risk < prev_risk {
                    visited.insert(edge.to_state, new_risk);
                    let mut new_path = path.clone();
                    new_path.push(edge.to_state);
                    let mut new_actions = actions.clone();
                    new_actions.push(edge.action.clone());
                    queue.push((edge.to_state, new_path, new_actions, new_risk, new_prob));
                }
            }
        }

        best_path.unwrap_or(SafePath {
            path: vec![start],
            actions: vec![],
            total_risk: 0.0,
            success_probability: 0.0,
            summary: "Ingen väg hittades till målet".to_string(),
        })
    }

    /// Bygg graf från temporal snapshots + actions
    pub fn build_from_history(snapshots: &[CausalSnapshotInput], actions: &[String]) -> Self {
        let mut graph = CausalGraph::new();

        let mut prev_state_id: Option<u32> = None;

        for (i, snap) in snapshots.iter().enumerate() {
            let state_id = graph.add_state(
                &snap.url,
                snap.node_count,
                snap.warning_count,
                snap.key_elements.clone(),
            );

            if let Some(prev) = prev_state_id {
                let action = actions
                    .get(i.saturating_sub(1))
                    .cloned()
                    .unwrap_or_else(|| "navigate".to_string());
                let action_type = infer_action_type(&action);
                graph.add_edge(prev, state_id, &action, action_type);
            }

            prev_state_id = Some(state_id);
        }

        if let Some(last) = prev_state_id {
            graph.current_state_id = Some(last);
        }

        graph
    }

    /// Serialisera till JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid causal graph: {}", e))
    }
}

/// Flernivå semantisk matchning av goal mot ett tillstånd
///
/// Tre nivåer:
/// 1. Direkt likhet (text_similarity) mot URL och key_elements
/// 2. Ordnivå: varje nyckelord i goal matchas mot varje element
/// 3. Kontextord: domänspecifika synonymer (kontakt↔telefon, köp↔pris, etc.)
fn semantic_goal_match(goal: &str, state: &CausalState) -> bool {
    let goal_lower = goal.to_lowercase();

    // Nivå 1: Direkt text_similarity (sänkt tröskel)
    if text_similarity(&goal_lower, &state.url) > 0.2 {
        return true;
    }
    if state
        .key_elements
        .iter()
        .any(|e| text_similarity(&goal_lower, e) > 0.25)
    {
        return true;
    }

    // Nivå 2: Ordnivå-matchning — splitta goal i ord, matcha mot element
    let goal_words: Vec<&str> = goal_lower
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-' || c == ':')
        .filter(|w| w.len() >= 3) // Ignorera korta ord (i, på, för, etc.)
        .collect();

    // Matcha goal-ord mot URL-segment
    let url_lower = state.url.to_lowercase();
    let url_word_matches = goal_words.iter().filter(|w| url_lower.contains(*w)).count();
    if url_word_matches >= 1 && !goal_words.is_empty() {
        return true;
    }

    // Matcha goal-ord mot key_elements
    // Navigeringselement (link:, button:) kräver striktare matchning
    // — de pekar på innehåll, de ÄR inte innehållet
    for element in &state.key_elements {
        let elem_lower = element.to_lowercase();
        let is_nav_element = elem_lower.starts_with("link:")
            || elem_lower.starts_with("button:")
            || elem_lower.starts_with("menuitem:");

        if is_nav_element {
            // Navigeringselement: kräv exakt text_similarity > 0.5
            // (dvs "link:Kontakta oss" matchar "kontakta oss" men inte "kontaktinformation")
            let label = elem_lower.split(':').nth(1).unwrap_or("");
            if text_similarity(&goal_lower, label) > 0.5 {
                return true;
            }
        } else {
            // Innehållselement (text:, heading:, etc.): ordnivå-matchning
            let element_word_matches = goal_words
                .iter()
                .filter(|w| elem_lower.contains(*w))
                .count();
            if element_word_matches >= 1 {
                return true;
            }
        }
    }

    // Nivå 3: Kontextord-mappning — domänspecifika synonymer
    let context_matches: &[(&[&str], &[&str])] = &[
        // Goal-ord → element-mönster som indikerar match
        (
            &["kontakt", "contact", "telefon", "phone", "ring"],
            &["0", "tel:", "phone", "mail", "@", "kontakt"],
        ),
        (
            &["epost", "email", "e-post", "mail"],
            &["@", "mail", "epost", "e-post"],
        ),
        (
            &["adress", "address", "besök"],
            &["gatan", "vägen", "plats", "torget", "adress"],
        ),
        (
            &["pris", "price", "cost", "kostnad"],
            &["kr", "sek", ":-", "price", "pris"],
        ),
        (
            &["köp", "buy", "purchase", "beställ"],
            &["cart", "varukorg", "köp", "buy", "lägg"],
        ),
        (
            &["nyhet", "news", "artikel"],
            &["nyhet", "news", "artikel", "publicerad"],
        ),
        (
            &["inlogg", "login", "logga"],
            &["login", "logga", "sign", "lösenord", "password"],
        ),
        (&["sök", "search"], &["sök", "search", "hitta", "find"]),
    ];

    let all_elements_lower: String = state
        .key_elements
        .iter()
        .map(|e| e.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    for (goal_patterns, element_patterns) in context_matches {
        let goal_has_pattern = goal_patterns.iter().any(|p| goal_lower.contains(p));
        if !goal_has_pattern {
            continue;
        }
        let element_has_pattern = element_patterns
            .iter()
            .any(|p| all_elements_lower.contains(p) || url_lower.contains(p));
        if element_has_pattern {
            return true;
        }
    }

    false
}

/// Beräkna match-poäng för ett goal mot ett tillstånd
///
/// Högre poäng = starkare match. Används för att rangordna goal-states
/// och undvika att start-staten alltid väljs som mål.
fn semantic_goal_score(goal: &str, state: &CausalState) -> f32 {
    let goal_lower = goal.to_lowercase();
    let mut score = 0.0f32;

    // URL-likhet
    let url_sim = text_similarity(&goal_lower, &state.url);
    score += url_sim * 0.3;

    // Key-element likhet (summera bästa matchning)
    let best_elem_sim = state
        .key_elements
        .iter()
        .map(|e| text_similarity(&goal_lower, e))
        .fold(0.0f32, f32::max);
    score += best_elem_sim * 0.3;

    // Kontextmatchning ger bonus
    let goal_words: Vec<&str> = goal_lower
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|w| w.len() >= 3)
        .collect();

    let all_elements_lower: String = state
        .key_elements
        .iter()
        .map(|e| e.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    // Kontextord-matchning bonus
    let context_matches: &[(&[&str], &[&str])] = &[
        (
            &["kontakt", "contact", "telefon", "phone"],
            &["0", "tel:", "phone", "mail", "@", "kontakt"],
        ),
        (
            &["epost", "email", "e-post", "mail"],
            &["@", "mail", "epost"],
        ),
        (
            &["adress", "address", "besök"],
            &["gatan", "vägen", "torget", "adress"],
        ),
        (
            &["pris", "price", "cost"],
            &["kr", "sek", ":-", "price", "pris"],
        ),
    ];

    let url_lower = state.url.to_lowercase();
    for (goal_patterns, element_patterns) in context_matches {
        let goal_has = goal_patterns.iter().any(|p| goal_lower.contains(p));
        let elem_has = element_patterns
            .iter()
            .any(|p| all_elements_lower.contains(p) || url_lower.contains(p));
        if goal_has && elem_has {
            score += 0.4; // Stark kontextbonus
        }
    }

    // Innehållselement (text:, heading:) med ord-matchning ger bonus
    for element in &state.key_elements {
        let elem_lower = element.to_lowercase();
        if !elem_lower.starts_with("link:") && !elem_lower.starts_with("button:") {
            let word_matches = goal_words
                .iter()
                .filter(|w| elem_lower.contains(*w))
                .count();
            score += word_matches as f32 * 0.1;
        }
    }

    score
}

/// Härleda action-typ från action-sträng
fn infer_action_type(action: &str) -> ActionType {
    let lower = action.to_lowercase();
    if lower.contains("click") || lower.contains("klick") {
        ActionType::Click
    } else if lower.contains("fill") || lower.contains("fyll") || lower.contains("type") {
        ActionType::Fill
    } else if lower.contains("extract") || lower.contains("hämta") {
        ActionType::Extract
    } else if lower.contains("wait") || lower.contains("vänta") {
        ActionType::Wait
    } else if lower.contains("verify") || lower.contains("verifiera") {
        ActionType::Verify
    } else {
        ActionType::Navigate
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = CausalGraph::new();
        assert!(graph.states.is_empty(), "Tom graf borde sakna tillstånd");
        assert!(graph.edges.is_empty(), "Tom graf borde sakna kanter");
    }

    #[test]
    fn test_add_state() {
        let mut graph = CausalGraph::new();
        let id = graph.add_state("https://shop.se", 10, 0, vec!["button:Köp".to_string()]);
        assert_eq!(id, 0, "Första tillståndet borde ha ID 0");
        assert_eq!(graph.states.len(), 1);
    }

    #[test]
    fn test_duplicate_state_increments_visits() {
        let mut graph = CausalGraph::new();
        let keys = vec!["button:Köp".to_string()];
        graph.add_state("https://shop.se", 10, 0, keys.clone());
        graph.add_state("https://shop.se", 10, 0, keys);
        assert_eq!(
            graph.states.len(),
            1,
            "Borde återanvända befintligt tillstånd"
        );
        assert_eq!(graph.states[0].visit_count, 2);
    }

    #[test]
    fn test_add_edge() {
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state("https://shop.se", 5, 0, vec![]);
        let s1 = graph.add_state("https://shop.se/kassa", 8, 0, vec![]);
        graph.add_edge(s0, s1, "click: Köp", ActionType::Click);

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].from_state, 0);
        assert_eq!(graph.edges[0].to_state, 1);
    }

    #[test]
    fn test_probability_normalization() {
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state("https://shop.se", 5, 0, vec![]);
        let s1 = graph.add_state("https://shop.se/a", 5, 0, vec![]);
        let s2 = graph.add_state("https://shop.se/b", 5, 0, vec![]);
        graph.add_edge(s0, s1, "click: A", ActionType::Click);
        graph.add_edge(s0, s2, "click: B", ActionType::Click);

        let total_prob: f32 = graph
            .edges
            .iter()
            .filter(|e| e.from_state == s0)
            .map(|e| e.probability)
            .sum();
        assert!(
            (total_prob - 1.0).abs() < 0.01,
            "Sannolikheter borde summera till 1.0, fick {}",
            total_prob
        );
    }

    #[test]
    fn test_predict_outcome_known() {
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state("https://shop.se", 5, 0, vec![]);
        let s1 = graph.add_state("https://shop.se/kassa", 8, 0, vec![]);
        graph.add_edge(s0, s1, "click: Köp", ActionType::Click);
        graph.current_state_id = Some(s0);

        let prediction = graph.predict_outcome("click: Köp", None);
        assert!(!prediction.outcomes.is_empty(), "Borde ha minst ett utfall");
        assert_eq!(prediction.outcomes[0].state_id, s1);
    }

    #[test]
    fn test_predict_outcome_unknown_action() {
        let graph = CausalGraph::new();
        let prediction = graph.predict_outcome("okänd action", None);
        assert!(
            prediction.outcomes.is_empty(),
            "Okänd action borde ge tomt resultat"
        );
        assert!(
            prediction.recommendation.contains("Okänd"),
            "Borde ge varning för okänd action"
        );
    }

    #[test]
    fn test_find_safest_path() {
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state("https://shop.se", 5, 0, vec!["button:Köp".to_string()]);
        let s1 = graph.add_state(
            "https://shop.se/kassa",
            8,
            0,
            vec!["button:Betala".to_string()],
        );
        let s2 = graph.add_state(
            "https://shop.se/bekräftelse",
            3,
            0,
            vec!["text:Tack".to_string()],
        );
        graph.add_edge(s0, s1, "click: Köp", ActionType::Click);
        graph.add_edge(s1, s2, "click: Betala", ActionType::Click);
        graph.current_state_id = Some(s0);

        let path = graph.find_safest_path("bekräftelse", 10);
        assert!(
            path.path.len() >= 2,
            "Borde hitta väg med minst 2 tillstånd"
        );
        assert!(
            path.success_probability > 0.0,
            "Borde ha positiv sannolikhet"
        );
    }

    #[test]
    fn test_find_safest_path_no_goal() {
        let graph = CausalGraph::new();
        let path = graph.find_safest_path("okänt mål", 10);
        assert!(
            path.summary.contains("Inget känt"),
            "Borde rapportera att mål saknas"
        );
    }

    #[test]
    fn test_semantic_goal_match_kontakt() {
        // BUG-6 regression: "kontaktinformation" borde matcha state med telefonnummer
        let state = CausalState {
            state_id: 1,
            url: "https://www.hjo.se/kontakt".to_string(),
            node_count: 500,
            warning_count: 0,
            key_elements: vec![
                "text:0503-350 00".to_string(),
                "text:kommunen@hjo.se".to_string(),
            ],
            visit_count: 1,
        };
        assert!(
            semantic_goal_match("Hitta kontaktinformation för Hjo kommun", &state),
            "Goal med 'kontakt' borde matcha state med telefonnummer och email"
        );
    }

    #[test]
    fn test_semantic_goal_match_url_word() {
        let state = CausalState {
            state_id: 0,
            url: "https://shop.se/checkout".to_string(),
            node_count: 10,
            warning_count: 0,
            key_elements: vec!["button:Betala".to_string()],
            visit_count: 1,
        };
        assert!(
            semantic_goal_match("gå till checkout", &state),
            "Goal med 'checkout' borde matcha URL med /checkout"
        );
    }

    #[test]
    fn test_semantic_goal_match_context_email() {
        let state = CausalState {
            state_id: 2,
            url: "https://example.se/about".to_string(),
            node_count: 50,
            warning_count: 0,
            key_elements: vec!["text:info@example.se".to_string()],
            visit_count: 1,
        };
        assert!(
            semantic_goal_match("hitta epost-adress", &state),
            "Goal med 'epost' borde matcha state med @-adress"
        );
    }

    #[test]
    fn test_semantic_goal_match_no_false_positive() {
        let state = CausalState {
            state_id: 0,
            url: "https://shop.se".to_string(),
            node_count: 5,
            warning_count: 0,
            key_elements: vec!["button:Köp".to_string()],
            visit_count: 1,
        };
        assert!(
            !semantic_goal_match("hitta kontaktinformation", &state),
            "Köp-knapp borde inte matcha kontaktinformation"
        );
    }

    #[test]
    fn test_find_safest_path_semantic_kontakt() {
        // BUG-6 end-to-end: find_safest_path borde hitta kontakt-state
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state(
            "https://www.hjo.se",
            2149,
            0,
            vec!["link:Kontakt".to_string(), "heading:Nyheter".to_string()],
        );
        let s1 = graph.add_state(
            "https://www.hjo.se/kontakt",
            500,
            0,
            vec![
                "text:0503-350 00".to_string(),
                "text:kommunen@hjo.se".to_string(),
            ],
        );
        graph.add_edge(s0, s1, "click link:Kontakt", ActionType::Click);
        graph.current_state_id = Some(s0);

        let path = graph.find_safest_path("Hitta kontaktinformation för Hjo kommun", 10);
        assert!(
            path.path.len() >= 2,
            "Borde hitta väg till kontakt-state, fick {:?}",
            path
        );
        assert!(
            path.success_probability > 0.0,
            "Borde ha positiv sannolikhet"
        );
        assert!(
            path.path.contains(&s1),
            "Vägen borde inkludera kontakt-state (s1={})",
            s1
        );
    }

    #[test]
    fn test_build_from_history() {
        let snapshots = vec![
            CausalSnapshotInput {
                url: "https://shop.se".to_string(),
                node_count: 5,
                warning_count: 0,
                key_elements: vec!["button:Köp".to_string()],
            },
            CausalSnapshotInput {
                url: "https://shop.se/kassa".to_string(),
                node_count: 8,
                warning_count: 0,
                key_elements: vec!["button:Betala".to_string()],
            },
        ];
        let actions = vec!["click: Köp".to_string()];

        let graph = CausalGraph::build_from_history(&snapshots, &actions);
        assert_eq!(graph.states.len(), 2, "Borde ha 2 tillstånd");
        assert_eq!(graph.edges.len(), 1, "Borde ha 1 kant");
    }

    #[test]
    fn test_risk_increases_with_warnings() {
        let mut graph = CausalGraph::new();
        let s0 = graph.add_state("https://safe.se", 5, 0, vec![]);
        let s1 = graph.add_state("https://risky.se", 5, 3, vec![]);
        graph.add_edge(s0, s1, "navigate", ActionType::Navigate);

        assert!(
            graph.edges[0].risk_score > 0.0,
            "Risk borde öka när varningar tillkommer"
        );
    }

    #[test]
    fn test_graph_serialization() {
        let mut graph = CausalGraph::new();
        graph.add_state("https://test.se", 5, 0, vec!["button:Test".to_string()]);

        let json = graph.to_json();
        let restored = CausalGraph::from_json(&json).expect("Borde gå att deserialisera");
        assert_eq!(restored.states.len(), 1);
        assert_eq!(restored.states[0].url, "https://test.se");
    }

    #[test]
    fn test_infer_action_type() {
        assert_eq!(infer_action_type("click: Köp"), ActionType::Click);
        assert_eq!(infer_action_type("fill email"), ActionType::Fill);
        assert_eq!(infer_action_type("extract price"), ActionType::Extract);
        assert_eq!(infer_action_type("navigate home"), ActionType::Navigate);
    }
}
