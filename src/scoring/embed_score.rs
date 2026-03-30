// Bottom-up Embedding Scoring
//
// Kör embedding-similarity enbart på survivors från HDC-pruning.
// Löv-noder scoreas direkt via embedding, föräldranoder ärver max(barn) × decay.
//
// Detta inverterar det nuvarande top-down-mönstret (Bugg B-fix):
// istället för att wrappers aggregerar barnens text och "stjäl" relevance,
// scoreas löv-noder först och föräldrarna ärver reducerat.

use std::collections::HashMap;

use crate::semantic::text_similarity_cached;
use crate::types::SemanticNode;

/// Decay-faktor: förälders relevance = max(barn) × PARENT_DECAY
const PARENT_DECAY: f32 = 0.75;

/// Max antal embedding-anrop (begränsa beräkningstid)
const MAX_EMBEDDING_CALLS: usize = 50;

/// Resultat från bottom-up scoring
#[derive(Debug, Clone)]
pub struct ScoredNode {
    pub id: u32,
    pub relevance: f32,
    pub role: String,
    pub label: String,
}

/// Score survivors bottom-up: löv-noder först, föräldrar ärver
///
/// `survivors` — (node_id, tfidf_score) från pipeline steg 1+2
/// `all_nodes` — platt map av alla noder i trädet
/// `goal` — agentens mål-sträng
/// `goal_words` — pre-tokeniserade goal-ord (för text_similarity_cached)
/// `goal_embedding` — pre-computed goal-vektor (Option, kan vara None utan embeddings-feature)
pub fn score_bottom_up(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
    goal: &str,
    goal_words: &[String],
    goal_embedding: Option<&[f32]>,
) -> Vec<ScoredNode> {
    let goal_lower = goal.to_lowercase();
    // Steg 1: Scorea alla survivors (löv-noder direkt, icke-löv väntar)
    let mut scores: HashMap<u32, f32> = HashMap::new();
    let mut embedding_calls = 0usize;

    // Först: scorea löv-noder
    for &(node_id, tfidf_score) in survivors {
        if let Some(info) = all_nodes.get(&node_id) {
            if info.is_leaf {
                let score = compute_node_score(
                    &info.label,
                    &info.role,
                    tfidf_score,
                    &goal_lower,
                    goal_words,
                    goal_embedding,
                    &mut embedding_calls,
                );
                scores.insert(node_id, score);
            }
        }
    }

    // Steg 2: Scorea icke-löv-noder bottom-up
    // Icke-löv: max(barn-score) × PARENT_DECAY, men minst sin egen text-score
    for &(node_id, tfidf_score) in survivors {
        if scores.contains_key(&node_id) {
            continue; // redan scoread (löv)
        }
        if let Some(info) = all_nodes.get(&node_id) {
            // Hitta max barn-score bland survivors
            let max_child_score = info
                .child_ids
                .iter()
                .filter_map(|cid| scores.get(cid))
                .copied()
                .fold(0.0f32, f32::max);

            // Egen text-score som fallback
            let own_score = compute_node_score(
                &info.label,
                &info.role,
                tfidf_score,
                &goal_lower,
                goal_words,
                goal_embedding,
                &mut embedding_calls,
            );

            // Ta max av (barn-arv, egen score)
            let inherited = max_child_score * PARENT_DECAY;
            scores.insert(node_id, own_score.max(inherited));
        }
    }

    // Steg 3: Samla resultat, dedup, och sortera
    let mut result: Vec<ScoredNode> = survivors
        .iter()
        .filter_map(|&(id, _)| {
            let info = all_nodes.get(&id)?;
            let mut relevance = scores.get(&id).copied().unwrap_or(0.0);

            // Leaf-link boost: löv-noder med roll link och lagom label-längd (30-200)
            // är typiska story-titlar, artikelrubriker, produktnamn — hög signal
            if info.is_leaf
                && info.role == "link"
                && info.label.len() >= 30
                && info.label.len() <= 200
            {
                relevance *= 1.15;
            }

            Some(ScoredNode {
                id,
                relevance: relevance.min(1.0),
                role: info.role.clone(),
                label: info.label.clone(),
            })
        })
        .collect();

    result.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));

    // Steg 4: Dedup — identiska labels → behåll bara den med högst score.
    // Wrappers på olika djup har ofta exakt samma label (aggregerad barntext).
    let mut seen_labels: std::collections::HashSet<String> = std::collections::HashSet::new();
    result.retain(|node| {
        // Normalisera label: ta första 80 tecken, trimma whitespace
        let key: String = node
            .label
            .chars()
            .take(80)
            .collect::<String>()
            .trim()
            .to_string();
        if key.is_empty() {
            return true; // behåll tomma labels (strukturella noder)
        }
        seen_labels.insert(key)
    });

    result
}

/// Beräkna score för en enskild nod
fn compute_node_score(
    label: &str,
    role: &str,
    tfidf_score: f32,
    goal_lower: &str,
    goal_words: &[String],
    goal_embedding: Option<&[f32]>,
    embedding_calls: &mut usize,
) -> f32 {
    if label.is_empty() {
        return 0.0;
    }

    // 1. Text-likhet (snabb word-overlap)
    let text_score = text_similarity_cached(goal_lower, goal_words, label);

    // 2. Embedding-förstärkning (om tillgänglig och budget kvar)
    let embed_score =
        if text_score < 0.8 && *embedding_calls < MAX_EMBEDDING_CALLS && label.len() > 3 {
            if let Some(goal_vec) = goal_embedding {
                *embedding_calls += 1;
                crate::embedding::similarity_with_vec(goal_vec, label).unwrap_or(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

    let semantic_score = text_score.max(embed_score);

    // 3. Roll-prioritet
    let role_score = SemanticNode::role_priority(role);

    // 4. Wrapper-penalty: aggregerade labels (lång text + strukturell roll)
    let is_structural = matches!(
        role,
        "generic" | "table" | "main" | "banner" | "complementary"
    );
    let label_penalty = if is_structural && label.len() > 200 {
        // Lång strukturell wrapper — hög penalty (döljer riktiga content-noder)
        0.20
    } else if label.len() > 500 {
        0.15
    } else if is_structural && label.len() > 100 {
        // Medellång wrapper — mildare
        0.10
    } else if label.len() > 300 {
        0.08
    } else {
        0.0
    };

    // 5. TF-IDF-boost: normaliserad TF-IDF bidrar till total score
    // Normalisera TF-IDF till [0, 1] — clamp vid 3.0 (empirisk max)
    let tfidf_norm = (tfidf_score / 3.0).min(1.0);

    // 6. Role-multiplier: referens-länkar nedprioriteras, faktanoder upprioriteras
    let role_mult = role_multiplier(role, label);

    // Viktat medelvärde: semantisk 45%, roll 25%, bm25 30%
    let raw = ((semantic_score * 0.45) + (role_score * 0.25) + (tfidf_norm * 0.30) - label_penalty)
        * role_mult;

    raw.clamp(0.0, 1.0)
}

/// Role-multiplier: nedprioritera referens-/nav-länkar, upprioritera faktanoder.
///
/// Wikipedia-sidor har tusentals referens-länktexter som råkar innehålla goal-
/// nyckelord och tränger undan infobox/table-noder med faktiska svar (Bugg G).
fn role_multiplier(role: &str, label: &str) -> f32 {
    match role {
        // Referens-länkar: citat-titlar, "See also", fotnoter
        "link" if label.starts_with('"') || label.starts_with('\u{201C}') => 0.4,
        // Korta nav-länkar (<40 tecken): "Home", "About", "Page 2"
        "link" if label.len() < 40 => 0.7,
        // Vanliga länkar — neutral
        "link" => 0.85,
        // Strukturerade faktanoder — boost
        "row" | "cell" | "definition" | "listitem" => 1.15,
        "data" => 1.2,
        // Tabellrubriker
        "columnheader" | "rowheader" => 1.1,
        // Headings — mild boost (ofta innehåller fråge-kontext)
        "heading" => 1.05,
        _ => 1.0,
    }
}

/// Info om en nod i det platta indexet
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub role: String,
    pub label: String,
    pub is_leaf: bool,
    pub child_ids: Vec<u32>,
    pub depth: u32,
}

/// Bygg platt index från semantiskt träd
pub fn build_node_index(nodes: &[SemanticNode]) -> HashMap<u32, NodeInfo> {
    let mut index = HashMap::new();
    build_index_recursive(nodes, &mut index, 0);
    index
}

fn build_index_recursive(nodes: &[SemanticNode], index: &mut HashMap<u32, NodeInfo>, depth: u32) {
    for node in nodes {
        let child_ids: Vec<u32> = node.children.iter().map(|c| c.id).collect();
        index.insert(
            node.id,
            NodeInfo {
                role: node.role.clone(),
                label: node.label.clone(),
                is_leaf: node.children.is_empty(),
                child_ids,
                depth,
            },
        );
        build_index_recursive(&node.children, index, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal_words(goal: &str) -> Vec<String> {
        goal.to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    #[test]
    fn test_leaf_scores_higher_than_wrapper() {
        // Simulera Bugg B: löv-nod med svar vs wrapper med aggregerad text
        let mut all_nodes = HashMap::new();
        all_nodes.insert(
            1,
            NodeInfo {
                role: "generic".into(),
                label: "Wrapper with lots of text about population and weather and cookies and many other topics that dilute the relevance".into(),
                is_leaf: false,
                child_ids: vec![2],
                depth: 0,
            },
        );
        all_nodes.insert(
            2,
            NodeInfo {
                role: "text".into(),
                label: "367924 inhabitants population count".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 1,
            },
        );

        let survivors = vec![(1, 0.5), (2, 1.5)];
        let goal = "population count";
        let goal_words = make_goal_words(goal);

        let scored = score_bottom_up(&survivors, &all_nodes, goal, &goal_words, None);

        assert!(scored.len() == 2, "Borde returnera 2 noder");
        // Löv-noden (id=2) borde rankas högst
        assert_eq!(
            scored[0].id, 2,
            "Löv-nod (id=2) borde rankas högre än wrapper (id=1)"
        );
        assert!(
            scored[0].relevance > scored[1].relevance,
            "Löv-nod borde ha högre relevance: löv={}, wrapper={}",
            scored[0].relevance,
            scored[1].relevance
        );
    }

    #[test]
    fn test_parent_inherits_from_children() {
        let mut all_nodes = HashMap::new();
        all_nodes.insert(
            1,
            NodeInfo {
                role: "generic".into(),
                label: "Section".into(),
                is_leaf: false,
                child_ids: vec![2],
                depth: 0,
            },
        );
        all_nodes.insert(
            2,
            NodeInfo {
                role: "button".into(),
                label: "population data download".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 1,
            },
        );

        let survivors = vec![(1, 0.1), (2, 1.0)];
        let goal = "population data";
        let goal_words = make_goal_words(goal);

        let scored = score_bottom_up(&survivors, &all_nodes, goal, &goal_words, None);

        let parent_score = scored.iter().find(|n| n.id == 1).map(|n| n.relevance);
        let child_score = scored.iter().find(|n| n.id == 2).map(|n| n.relevance);

        assert!(
            parent_score.is_some() && child_score.is_some(),
            "Båda noder borde ha scores"
        );
        // Parent borde ärva max(child) * 0.75, inte vara 0
        assert!(
            parent_score.unwrap() > 0.0,
            "Parent borde ärva score från barn, fick {}",
            parent_score.unwrap()
        );
    }

    #[test]
    fn test_empty_label_gets_zero() {
        let mut all_nodes = HashMap::new();
        all_nodes.insert(
            1,
            NodeInfo {
                role: "generic".into(),
                label: "".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 0,
            },
        );

        let survivors = vec![(1, 0.5)];
        let scored = score_bottom_up(&survivors, &all_nodes, "test", &["test".into()], None);

        assert!(
            scored[0].relevance < 0.3,
            "Tom label borde ge låg score, fick {}",
            scored[0].relevance
        );
    }

    #[test]
    fn test_long_label_penalized() {
        let mut all_nodes = HashMap::new();
        let long_label = "population ".repeat(100); // > 500 chars
        all_nodes.insert(
            1,
            NodeInfo {
                role: "text".into(),
                label: long_label,
                is_leaf: true,
                child_ids: vec![],
                depth: 0,
            },
        );
        all_nodes.insert(
            2,
            NodeInfo {
                role: "text".into(),
                label: "population statistics data".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 0,
            },
        );

        let survivors = vec![(1, 1.0), (2, 1.0)];
        let goal = "population";
        let goal_words = make_goal_words(goal);

        let scored = score_bottom_up(&survivors, &all_nodes, goal, &goal_words, None);

        // Nod 2 (kort label) borde rankas högre pga label-penalty på nod 1
        let node1 = scored.iter().find(|n| n.id == 1).unwrap();
        let node2 = scored.iter().find(|n| n.id == 2).unwrap();
        assert!(
            node2.relevance >= node1.relevance,
            "Kort label borde rankas högre: kort={}, lång={}",
            node2.relevance,
            node1.relevance
        );
    }

    #[test]
    fn test_build_node_index() {
        let tree = vec![SemanticNode {
            id: 1,
            role: "text".into(),
            label: "Root".into(),
            children: vec![
                SemanticNode {
                    id: 2,
                    role: "button".into(),
                    label: "Click".into(),
                    children: vec![],
                    ..SemanticNode::default()
                },
                SemanticNode {
                    id: 3,
                    role: "link".into(),
                    label: "More".into(),
                    children: vec![],
                    ..SemanticNode::default()
                },
            ],
            ..SemanticNode::default()
        }];

        let index = build_node_index(&tree);
        assert_eq!(index.len(), 3, "Borde indexera 3 noder");
        assert!(!index[&1].is_leaf, "Nod 1 borde vara icke-löv");
        assert!(index[&2].is_leaf, "Nod 2 borde vara löv");
        assert_eq!(index[&1].child_ids, vec![2, 3], "Nod 1 borde ha barn 2,3");
    }
}
