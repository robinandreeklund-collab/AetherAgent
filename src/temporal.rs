/// Temporal Memory & Adversarial Modeling – Fas 5
///
/// Tidsseriebaserad sida-spårning och prediktivt injektionsförsvar.
///
/// Pipeline:
/// 1. Spara snapshots av semantiska träd med tidsstämplar
/// 2. Detektera anomala förändringar (ny injection, plötslig strukturändring)
/// 3. Beräkna nod-volatilitet (hur ofta varje nod ändras)
/// 4. Prediktivt försvar: flagga noder som beter sig misstänkt över tid
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::diff;
#[cfg(test)]
use crate::types::{InjectionWarning, WarningSeverity};
use crate::types::{SemanticDelta, SemanticTree};

// ─── Types ──────────────────────────────────────────────────────────────────

/// En tidsstämplad ögonblicksbild av sidtillståndet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSnapshot {
    /// Steg-index i sekvensen
    pub step: u32,
    /// Tidsstämpel (ms sedan epoch)
    pub timestamp_ms: u64,
    /// URL vid detta tillfälle
    pub url: String,
    /// Antal noder i trädet
    pub node_count: u32,
    /// Antal injection-varningar
    pub warning_count: u32,
    /// Delta sedan föregående snapshot (None för första)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<SemanticDelta>,
}

/// Volatilitetsmätning per nod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeVolatility {
    /// Nod-ID
    pub node_id: u32,
    /// Roll
    pub role: String,
    /// Label vid senaste observation
    pub label: String,
    /// Antal gånger noden ändrats
    pub change_count: u32,
    /// Totalt antal observationer
    pub observation_count: u32,
    /// Volatilitet (0.0–1.0)
    pub volatility: f32,
}

/// Adversarial pattern – ett mönster som upprepas misstänkt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdversarialPattern {
    /// Vilken typ av misstänkt beteende
    pub pattern_type: AdversarialType,
    /// Beskrivning
    pub description: String,
    /// Konfidensgrad (0.0–1.0)
    pub confidence: f32,
    /// Vilka steg det gäller
    pub affected_steps: Vec<u32>,
    /// Relaterade nod-IDn
    pub affected_node_ids: Vec<u32>,
}

/// Typ av adversarial beteende
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdversarialType {
    /// Injection-varningar ökar successivt
    EscalatingInjection,
    /// Nya injection-mönster introduceras gradvis
    GradualInjection,
    /// Hög volatilitet i text-noder (potentiell injection-rotation)
    SuspiciousVolatility,
    /// Nod med plötsligt ändrad trust-level
    TrustLevelShift,
    /// Strukturell manipulation (noder läggs till/tas bort misstänkt)
    StructuralManipulation,
}

/// Huvudresultat: temporal analys av en sekvens av sidtillstånd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalAnalysis {
    /// Alla snapshots i sekvensen
    pub snapshots: Vec<TemporalSnapshot>,
    /// Nod-volatilitet (sorterad, mest volatila först)
    pub node_volatility: Vec<NodeVolatility>,
    /// Detekterade adversarial patterns
    pub adversarial_patterns: Vec<AdversarialPattern>,
    /// Total risk-poäng (0.0–1.0)
    pub risk_score: f32,
    /// Sammanfattning
    pub summary: String,
    /// Analystid i ms
    pub analysis_time_ms: u64,
}

/// Temporal Memory – stateless behållare som skickas över WASM-gränsen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalMemory {
    /// Historiska snapshots
    pub snapshots: Vec<TemporalSnapshot>,
    /// Senaste fullständiga träd-JSON (för diff mot nästa)
    pub last_tree_json: Option<String>,
    /// Ackumulerad varningshistorik per nod
    pub warning_history: HashMap<u32, u32>,
    /// Ackumulerad ändringshistorik per nod
    pub change_history: HashMap<u32, u32>,
    /// Observationsräknare per nod
    pub observation_history: HashMap<u32, u32>,
    /// Node role/label cache för volatility output
    pub node_labels: HashMap<u32, (String, String)>,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl TemporalMemory {
    /// Skapa nytt tomt temporal memory
    pub fn new() -> Self {
        TemporalMemory {
            snapshots: vec![],
            last_tree_json: None,
            warning_history: HashMap::new(),
            change_history: HashMap::new(),
            observation_history: HashMap::new(),
            node_labels: HashMap::new(),
        }
    }

    /// Lägg till en snapshot (nytt sidtillstånd)
    pub fn add_snapshot(&mut self, tree: &SemanticTree, tree_json: &str, timestamp_ms: u64) {
        let step = self.snapshots.len() as u32;

        // Beräkna delta mot föregående
        let delta = if let Some(ref prev_json) = self.last_tree_json {
            diff::compute_delta(prev_json, tree_json).ok()
        } else {
            None
        };

        // Uppdatera varningshistorik
        for warning in &tree.injection_warnings {
            *self.warning_history.entry(warning.node_id).or_insert(0) += 1;
        }

        // Uppdatera nod-observations- och ändringshistorik
        let all_nodes = collect_node_ids(&tree.nodes);
        for (node_id, role, label) in &all_nodes {
            *self.observation_history.entry(*node_id).or_insert(0) += 1;
            self.node_labels
                .insert(*node_id, (role.clone(), label.clone()));
        }

        // Räkna ändringar från delta
        if let Some(ref d) = delta {
            for change in &d.changes {
                *self.change_history.entry(change.node_id).or_insert(0) += 1;
            }
        }

        let snapshot = TemporalSnapshot {
            step,
            timestamp_ms,
            url: tree.url.clone(),
            node_count: count_all_nodes(&tree.nodes),
            warning_count: tree.injection_warnings.len() as u32,
            delta,
        };

        self.snapshots.push(snapshot);
        self.last_tree_json = Some(tree_json.to_string());
    }

    /// Kör full temporal analys
    pub fn analyze(&self) -> TemporalAnalysis {
        let volatility = self.compute_volatility();
        let patterns = self.detect_adversarial_patterns();

        let risk_score = compute_risk_score(&patterns);
        let summary = build_summary(&self.snapshots, &volatility, &patterns, risk_score);

        TemporalAnalysis {
            snapshots: self.snapshots.clone(),
            node_volatility: volatility,
            adversarial_patterns: patterns,
            risk_score,
            summary,
            analysis_time_ms: 0, // sätts av anroparen
        }
    }

    /// Beräkna volatilitet per nod
    fn compute_volatility(&self) -> Vec<NodeVolatility> {
        let mut result: Vec<NodeVolatility> = self
            .observation_history
            .iter()
            .map(|(node_id, &obs)| {
                let changes = self.change_history.get(node_id).copied().unwrap_or(0);
                let volatility = if obs > 0 {
                    changes as f32 / obs as f32
                } else {
                    0.0
                };
                let (role, label) = self.node_labels.get(node_id).cloned().unwrap_or_default();

                NodeVolatility {
                    node_id: *node_id,
                    role,
                    label,
                    change_count: changes,
                    observation_count: obs,
                    volatility,
                }
            })
            .filter(|v| v.observation_count > 1 || v.change_count > 0)
            .collect();

        // Sortera: mest volatila först
        result.sort_by(|a, b| b.volatility.total_cmp(&a.volatility));
        result
    }

    /// Detektera adversarial patterns i historiken
    fn detect_adversarial_patterns(&self) -> Vec<AdversarialPattern> {
        let mut patterns = Vec::new();

        // 1. Eskalerande injection: varningar ökar över tid
        self.detect_escalating_injection(&mut patterns);

        // 2. Gradvis injection: nya varningar i noder som var rena
        self.detect_gradual_injection(&mut patterns);

        // 3. Misstänkt volatilitet: text-noder ändras ovanligt ofta
        self.detect_suspicious_volatility(&mut patterns);

        // 4. Strukturell manipulation: plötsliga tillägg/borttagningar
        self.detect_structural_manipulation(&mut patterns);

        patterns
    }

    /// Eskalerande injection: varningsantal ökar monotont
    fn detect_escalating_injection(&self, patterns: &mut Vec<AdversarialPattern>) {
        if self.snapshots.len() < 3 {
            return;
        }

        let warning_counts: Vec<u32> = self.snapshots.iter().map(|s| s.warning_count).collect();
        let mut increasing_steps = Vec::new();
        let mut prev = 0u32;

        for (i, &count) in warning_counts.iter().enumerate() {
            if count > prev && count > 0 {
                increasing_steps.push(i as u32);
            }
            prev = count;
        }

        // Om varningarna ökar i minst 3 steg, flagga
        if increasing_steps.len() >= 3 {
            let last_count = warning_counts.last().copied().unwrap_or(0);
            let confidence = (increasing_steps.len() as f32 / self.snapshots.len() as f32).min(1.0);

            patterns.push(AdversarialPattern {
                pattern_type: AdversarialType::EscalatingInjection,
                description: format!(
                    "Injection-varningar eskalerar: {} till {} över {} steg",
                    warning_counts.first().copied().unwrap_or(0),
                    last_count,
                    increasing_steps.len()
                ),
                confidence,
                affected_steps: increasing_steps,
                affected_node_ids: self.warning_history.keys().copied().collect(),
            });
        }
    }

    /// Gradvis injection: noder som var rena får varningar
    fn detect_gradual_injection(&self, patterns: &mut Vec<AdversarialPattern>) {
        // Hitta noder som först observerades utan varningar men sedan fick varningar
        for (&node_id, &warning_count) in &self.warning_history {
            let obs = self.observation_history.get(&node_id).copied().unwrap_or(0);
            if obs > 2 && warning_count > 0 && warning_count < obs {
                // Noden var ren i början men fick varningar senare
                let confidence = (warning_count as f32 / obs as f32).min(0.9);
                if confidence > 0.2 {
                    patterns.push(AdversarialPattern {
                        pattern_type: AdversarialType::GradualInjection,
                        description: format!(
                            "Nod {} fick injection-varningar efter {} rena observationer",
                            node_id,
                            obs - warning_count
                        ),
                        confidence,
                        affected_steps: vec![],
                        affected_node_ids: vec![node_id],
                    });
                }
            }
        }
    }

    /// Misstänkt volatilitet: text ändras ovanligt ofta
    fn detect_suspicious_volatility(&self, patterns: &mut Vec<AdversarialPattern>) {
        let threshold = 0.7; // > 70% av observationer med ändringar

        let suspicious_nodes: Vec<u32> = self
            .change_history
            .iter()
            .filter(|(&node_id, &changes)| {
                let obs = self.observation_history.get(&node_id).copied().unwrap_or(0);
                obs >= 3 && (changes as f32 / obs as f32) > threshold
            })
            .map(|(&id, _)| id)
            .collect();

        if !suspicious_nodes.is_empty() {
            patterns.push(AdversarialPattern {
                pattern_type: AdversarialType::SuspiciousVolatility,
                description: format!(
                    "{} nod(er) med >{}% volatilitet – potentiell injection-rotation",
                    suspicious_nodes.len(),
                    (threshold * 100.0) as u32
                ),
                confidence: 0.6,
                affected_steps: vec![],
                affected_node_ids: suspicious_nodes,
            });
        }
    }

    /// Strukturell manipulation: stora tillägg/borttagningar
    fn detect_structural_manipulation(&self, patterns: &mut Vec<AdversarialPattern>) {
        let mut suspicious_steps = Vec::new();

        for snapshot in &self.snapshots {
            if let Some(ref delta) = snapshot.delta {
                let total_changes = delta.changes.len() as u32;
                let total_before = delta.total_nodes_before;

                // Om >50% av noder ändras i ett steg → misstänkt
                if total_before > 0 && total_changes as f32 / total_before as f32 > 0.5 {
                    suspicious_steps.push(snapshot.step);
                }
            }
        }

        if !suspicious_steps.is_empty() {
            patterns.push(AdversarialPattern {
                pattern_type: AdversarialType::StructuralManipulation,
                description: format!(
                    "{} steg med >50% strukturell förändring",
                    suspicious_steps.len()
                ),
                confidence: 0.5,
                affected_steps: suspicious_steps,
                affected_node_ids: vec![],
            });
        }
    }

    /// Serialisera till JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid temporal memory: {}", e))
    }
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Samla alla nod-IDn rekursivt
fn collect_node_ids(nodes: &[crate::types::SemanticNode]) -> Vec<(u32, String, String)> {
    let mut result = Vec::new();
    for node in nodes {
        result.push((node.id, node.role.clone(), node.label.clone()));
        result.extend(collect_node_ids(&node.children));
    }
    result
}

/// Räkna totalt antal noder
fn count_all_nodes(nodes: &[crate::types::SemanticNode]) -> u32 {
    let mut count = nodes.len() as u32;
    for node in nodes {
        count += count_all_nodes(&node.children);
    }
    count
}

/// Beräkna risk-poäng baserat på detekterade patterns
fn compute_risk_score(patterns: &[AdversarialPattern]) -> f32 {
    if patterns.is_empty() {
        return 0.0;
    }

    let weighted_sum: f32 = patterns
        .iter()
        .map(|p| {
            let type_weight = match p.pattern_type {
                AdversarialType::EscalatingInjection => 0.9,
                AdversarialType::GradualInjection => 0.8,
                AdversarialType::SuspiciousVolatility => 0.5,
                AdversarialType::TrustLevelShift => 0.7,
                AdversarialType::StructuralManipulation => 0.4,
            };
            type_weight * p.confidence
        })
        .sum();

    (weighted_sum / patterns.len() as f32).min(1.0)
}

/// Bygg sammanfattning
fn build_summary(
    snapshots: &[TemporalSnapshot],
    volatility: &[NodeVolatility],
    patterns: &[AdversarialPattern],
    risk_score: f32,
) -> String {
    let volatile_count = volatility.iter().filter(|v| v.volatility > 0.5).count();
    let risk_level = if risk_score > 0.7 {
        "HÖG"
    } else if risk_score > 0.3 {
        "MEDIUM"
    } else {
        "LÅG"
    };

    format!(
        "{} snapshots, {} volatila noder, {} adversarial patterns, risk: {} ({:.0}%)",
        snapshots.len(),
        volatile_count,
        patterns.len(),
        risk_level,
        risk_score * 100.0
    )
}

/// Prediktera sidtillstånd baserat på historik
pub fn predict_next_state(memory: &TemporalMemory) -> PredictedState {
    let last_snapshot = memory.snapshots.last();
    let expected_warnings = if memory.snapshots.len() >= 2 {
        // Trenden: ökar varningarna?
        let recent: Vec<u32> = memory
            .snapshots
            .iter()
            .rev()
            .take(3)
            .map(|s| s.warning_count)
            .collect();
        let avg = recent.iter().sum::<u32>() as f32 / recent.len() as f32;
        avg.ceil() as u32
    } else {
        0
    };

    let volatile_nodes: Vec<u32> = memory
        .change_history
        .iter()
        .filter(|(&node_id, &changes)| {
            let obs = memory
                .observation_history
                .get(&node_id)
                .copied()
                .unwrap_or(0);
            obs >= 2 && changes as f32 / obs as f32 > 0.5
        })
        .map(|(&id, _)| id)
        .collect();

    PredictedState {
        expected_node_count: last_snapshot.map(|s| s.node_count).unwrap_or(0),
        expected_warning_count: expected_warnings,
        likely_changed_nodes: volatile_nodes,
        confidence: if memory.snapshots.len() >= 3 {
            0.7
        } else {
            0.3
        },
    }
}

/// Predikterat sidtillstånd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedState {
    pub expected_node_count: u32,
    pub expected_warning_count: u32,
    pub likely_changed_nodes: Vec<u32>,
    pub confidence: f32,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SemanticNode, SemanticTree};

    fn make_tree(
        url: &str,
        nodes: Vec<SemanticNode>,
        warnings: Vec<InjectionWarning>,
    ) -> SemanticTree {
        SemanticTree {
            url: url.to_string(),
            title: "Test".to_string(),
            goal: "test".to_string(),
            nodes,
            injection_warnings: warnings,
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
        }
    }

    fn make_node(id: u32, role: &str, label: &str) -> SemanticNode {
        SemanticNode::new(id, role, label)
    }

    #[test]
    fn test_temporal_memory_empty() {
        let mem = TemporalMemory::new();
        let analysis = mem.analyze();
        assert_eq!(analysis.snapshots.len(), 0);
        assert_eq!(analysis.risk_score, 0.0);
        assert!(analysis.adversarial_patterns.is_empty());
    }

    #[test]
    fn test_temporal_memory_single_snapshot() {
        let mut mem = TemporalMemory::new();
        let tree = make_tree(
            "https://shop.se",
            vec![make_node(1, "button", "Köp")],
            vec![],
        );
        let json = serde_json::to_string(&tree).unwrap_or_default();
        mem.add_snapshot(&tree, &json, 1000);

        let analysis = mem.analyze();
        assert_eq!(analysis.snapshots.len(), 1);
        assert_eq!(analysis.snapshots[0].node_count, 1);
        assert_eq!(analysis.risk_score, 0.0);
    }

    #[test]
    fn test_temporal_memory_roundtrip() {
        let mut mem = TemporalMemory::new();
        let tree = make_tree(
            "https://shop.se",
            vec![make_node(1, "button", "Köp")],
            vec![],
        );
        let json = serde_json::to_string(&tree).unwrap_or_default();
        mem.add_snapshot(&tree, &json, 1000);

        let serialized = mem.to_json();
        let restored = TemporalMemory::from_json(&serialized).expect("Borde kunna deserialisera");
        assert_eq!(restored.snapshots.len(), 1);
    }

    #[test]
    fn test_escalating_injection_detection() {
        let mut mem = TemporalMemory::new();

        // 5 steg med ökande antal varningar
        for i in 0..5 {
            let warnings: Vec<InjectionWarning> = (0..i)
                .map(|w| InjectionWarning {
                    node_id: w,
                    reason: "test".to_string(),
                    severity: WarningSeverity::High,
                    raw_text: "test".to_string(),
                })
                .collect();
            let tree = make_tree(
                "https://evil.com",
                vec![make_node(1, "text", &format!("Steg {}", i))],
                warnings,
            );
            let json = serde_json::to_string(&tree).unwrap_or_default();
            mem.add_snapshot(&tree, &json, 1000 + i as u64 * 1000);
        }

        let analysis = mem.analyze();
        let escalating = analysis
            .adversarial_patterns
            .iter()
            .any(|p| p.pattern_type == AdversarialType::EscalatingInjection);
        assert!(escalating, "Borde detektera eskalerande injection");
        assert!(analysis.risk_score > 0.0, "Risk-poäng borde vara > 0");
    }

    #[test]
    fn test_safe_sequence_no_patterns() {
        let mut mem = TemporalMemory::new();

        // 5 steg utan varningar eller ändringar
        for i in 0..5 {
            let tree = make_tree(
                "https://safe.com",
                vec![make_node(1, "button", "Köp"), make_node(2, "link", "Info")],
                vec![],
            );
            let json = serde_json::to_string(&tree).unwrap_or_default();
            mem.add_snapshot(&tree, &json, 1000 + i as u64 * 1000);
        }

        let analysis = mem.analyze();
        assert!(
            analysis.adversarial_patterns.is_empty(),
            "Säker sekvens borde inte ha patterns"
        );
        assert_eq!(analysis.risk_score, 0.0);
    }

    #[test]
    fn test_prediction() {
        let mut mem = TemporalMemory::new();
        for i in 0..4 {
            let tree = make_tree(
                "https://shop.se",
                vec![make_node(1, "button", "Köp")],
                vec![],
            );
            let json = serde_json::to_string(&tree).unwrap_or_default();
            mem.add_snapshot(&tree, &json, 1000 + i as u64 * 1000);
        }

        let pred = predict_next_state(&mem);
        assert_eq!(pred.expected_node_count, 1);
        assert_eq!(pred.expected_warning_count, 0);
        assert!(
            pred.confidence > 0.5,
            "Borde ha hög konfidens med 4 snapshots"
        );
    }

    #[test]
    fn test_volatility_tracking() {
        let mut mem = TemporalMemory::new();

        // Skapa 4 snapshots med ändrad nod
        for i in 0..4 {
            let label = format!("Pris: {} kr", 100 + i * 10);
            let tree = make_tree(
                "https://shop.se",
                vec![make_node(1, "text", &label)],
                vec![],
            );
            let json = serde_json::to_string(&tree).unwrap_or_default();
            mem.add_snapshot(&tree, &json, 1000 + i as u64 * 1000);
        }

        let analysis = mem.analyze();
        // Nod 1 borde ha hög volatilitet (ändras varje steg)
        let vol = analysis.node_volatility.iter().find(|v| v.node_id == 1);
        assert!(vol.is_some(), "Borde ha volatilitet för nod 1");
    }
}
