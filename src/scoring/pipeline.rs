// ScoringPipeline — orchestrerar de tre stegen
//
// 1. TF-IDF kandidatretrieval (~0.05ms)
// 2. HDC pruning (~0.1ms)
// 3. Embedding bottom-up scoring (~2-5ms)
//
// Total query-tid: ~3-5ms istället för ~50ms med single-pass embedding.

use std::collections::HashMap;
use std::time::Instant;

use crate::types::SemanticNode;

use super::embed_score::{self, build_node_index, ScoredNode};
use super::hdc::{self, HdcTree};
use super::tfidf::{self, TfIdfIndex};

/// Konfiguration för hybrid-pipelinen
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Max antal TF-IDF-kandidater (steg 1)
    pub tfidf_top_k: usize,
    /// HDC threshold-multiplikator (lägre = fler passerar)
    pub hdc_threshold: f32,
    /// Använd adaptiv HDC threshold per nod
    pub adaptive_hdc: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        PipelineConfig {
            tfidf_top_k: 300,
            hdc_threshold: 0.02,
            adaptive_hdc: true,
        }
    }
}

/// Tidsmätningar för pipelinen
#[derive(Debug, Clone, Default)]
pub struct PipelineTimings {
    pub build_tfidf_us: u64,
    pub build_hdc_us: u64,
    pub query_tfidf_us: u64,
    pub prune_hdc_us: u64,
    pub score_embed_us: u64,
    pub total_us: u64,
    pub tfidf_candidates: usize,
    pub hdc_survivors: usize,
    pub final_scored: usize,
}

/// Resultat från hela pipelinen
pub struct PipelineResult {
    pub scored_nodes: Vec<ScoredNode>,
    pub timings: PipelineTimings,
}

/// Hybrid scoring pipeline
pub struct ScoringPipeline;

impl ScoringPipeline {
    /// Kör hela hybrid-pipelinen: TF-IDF → HDC → Embedding
    ///
    /// Returnerar scorade noder sorterade efter relevance (högst först).
    pub fn run(
        tree_nodes: &[SemanticNode],
        goal: &str,
        goal_embedding: Option<&[f32]>,
        config: &PipelineConfig,
    ) -> PipelineResult {
        let pipeline_start = Instant::now();
        let mut timings = PipelineTimings::default();

        // Pre-compute goal words
        let goal_lower = goal.to_lowercase();
        let goal_words: Vec<String> = goal_lower
            .split_whitespace()
            .filter(|s| s.len() > 2)
            .map(String::from)
            .collect();

        // Build-fas: TF-IDF index
        let t0 = Instant::now();
        let flat_nodes = tfidf::flatten_tree(tree_nodes);
        let tfidf_index = TfIdfIndex::build(&flat_nodes);
        timings.build_tfidf_us = t0.elapsed().as_micros() as u64;

        // Build-fas: HDC tree
        let t1 = Instant::now();
        let hdc_tree = HdcTree::build(tree_nodes);
        timings.build_hdc_us = t1.elapsed().as_micros() as u64;

        // Build: node index (platt map för bottom-up scoring)
        let node_index = build_node_index(tree_nodes);

        // Steg 1: TF-IDF kandidatretrieval
        let t2 = Instant::now();
        let candidates = tfidf_index.query(goal, config.tfidf_top_k);
        timings.query_tfidf_us = t2.elapsed().as_micros() as u64;
        timings.tfidf_candidates = candidates.len();

        // Om TF-IDF ger 0 kandidater: fallback till alla noder (med bas-score)
        let candidates = if candidates.is_empty() {
            flat_nodes
                .iter()
                .map(|&(id, _)| (id, 0.1f32))
                .collect::<Vec<_>>()
        } else {
            candidates
        };

        // Steg 2: HDC pruning
        let t3 = Instant::now();
        let goal_hv = HdcTree::project_goal(goal);
        let survivors = if config.adaptive_hdc {
            // Adaptiv: filtrera per nod baserat på roll + djup
            candidates
                .iter()
                .filter(|(id, _)| {
                    if let Some(info) = node_index.get(id) {
                        let threshold = hdc::adaptive_threshold(&info.role, info.depth);
                        hdc_tree
                            .node_similarity(*id, &goal_hv)
                            .map(|sim| sim >= threshold)
                            .unwrap_or(true)
                    } else {
                        true
                    }
                })
                .copied()
                .collect::<Vec<_>>()
        } else {
            hdc_tree.prune(&candidates, &goal_hv, config.hdc_threshold)
        };
        timings.prune_hdc_us = t3.elapsed().as_micros() as u64;
        timings.hdc_survivors = survivors.len();

        // Steg 3: Bottom-up embedding scoring
        let t4 = Instant::now();
        let scored = embed_score::score_bottom_up(
            &survivors,
            &node_index,
            goal,
            &goal_words,
            goal_embedding,
        );
        timings.score_embed_us = t4.elapsed().as_micros() as u64;
        timings.final_scored = scored.len();

        timings.total_us = pipeline_start.elapsed().as_micros() as u64;

        PipelineResult {
            scored_nodes: scored,
            timings,
        }
    }

    /// Applicera top_n på scorade noder
    pub fn apply_top_n(scored: Vec<ScoredNode>, top_n: Option<usize>) -> Vec<ScoredNode> {
        match top_n {
            Some(n) => scored.into_iter().take(n).collect(),
            None => scored,
        }
    }
}

/// Applicera pipeline-scores tillbaka på SemanticNodes i trädet
pub fn apply_scores_to_tree(nodes: &mut [SemanticNode], scores: &HashMap<u32, f32>) {
    for node in nodes.iter_mut() {
        if let Some(&score) = scores.get(&node.id) {
            node.relevance = score;
        }
        apply_scores_to_tree(&mut node.children, scores);
    }
}

/// Konvertera ScoredNode-lista till score-map
pub fn scores_to_map(scored: &[ScoredNode]) -> HashMap<u32, f32> {
    scored.iter().map(|s| (s.id, s.relevance)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SemanticNode;

    fn make_test_tree() -> Vec<SemanticNode> {
        vec![
            SemanticNode {
                id: 1,
                role: "generic".into(),
                label: "Main content wrapper about population data".into(),
                children: vec![
                    SemanticNode {
                        id: 2,
                        role: "heading".into(),
                        label: "Population Statistics".into(),
                        children: vec![],
                        ..SemanticNode::default()
                    },
                    SemanticNode {
                        id: 3,
                        role: "text".into(),
                        label: "367924 inhabitants population count".into(),
                        children: vec![],
                        ..SemanticNode::default()
                    },
                    SemanticNode {
                        id: 4,
                        role: "button".into(),
                        label: "Download population report".into(),
                        children: vec![],
                        ..SemanticNode::default()
                    },
                ],
                ..SemanticNode::default()
            },
            SemanticNode {
                id: 5,
                role: "navigation".into(),
                label: "Cookie settings privacy terms".into(),
                children: vec![SemanticNode {
                    id: 6,
                    role: "button".into(),
                    label: "Accept cookies".into(),
                    children: vec![],
                    ..SemanticNode::default()
                }],
                ..SemanticNode::default()
            },
        ]
    }

    #[test]
    fn test_pipeline_end_to_end() {
        let tree = make_test_tree();
        let config = PipelineConfig::default();

        let result = ScoringPipeline::run(&tree, "population statistics", None, &config);

        assert!(
            !result.scored_nodes.is_empty(),
            "Pipelinen borde returnera scorade noder"
        );

        // Nod 3 (löv med "population count") borde rankas högt
        let top_3: Vec<u32> = result.scored_nodes.iter().take(3).map(|n| n.id).collect();
        assert!(
            top_3.contains(&3) || top_3.contains(&2),
            "Nod med 'population' borde vara bland topp 3, fick: {:?}",
            top_3
        );

        // Cookie-noden borde vara lägre
        if let Some(cookie_pos) = result.scored_nodes.iter().position(|n| n.id == 6) {
            if let Some(pop_pos) = result.scored_nodes.iter().position(|n| n.id == 3) {
                assert!(
                    pop_pos < cookie_pos,
                    "Population-nod borde rankas före cookie-nod"
                );
            }
        }
    }

    #[test]
    fn test_pipeline_timings() {
        let tree = make_test_tree();
        let config = PipelineConfig::default();

        let result = ScoringPipeline::run(&tree, "population", None, &config);

        assert!(result.timings.total_us > 0, "Timings borde registreras");
        assert!(
            result.timings.tfidf_candidates > 0 || result.timings.hdc_survivors > 0,
            "Borde ha kandidater eller survivors"
        );
    }

    #[test]
    fn test_pipeline_top_n() {
        let tree = make_test_tree();
        let config = PipelineConfig::default();

        let result = ScoringPipeline::run(&tree, "population", None, &config);
        let top_2 = ScoringPipeline::apply_top_n(result.scored_nodes, Some(2));

        assert!(top_2.len() <= 2, "top_n=2 borde returnera max 2 noder");
    }

    #[test]
    fn test_pipeline_empty_goal() {
        let tree = make_test_tree();
        let config = PipelineConfig::default();

        // Tom goal → TF-IDF hittar inget → fallback till alla noder
        let result = ScoringPipeline::run(&tree, "", None, &config);

        // Borde inte krascha, och returnera noder (via fallback)
        assert!(
            !result.scored_nodes.is_empty(),
            "Tom goal borde fortfarande returnera noder via fallback"
        );
    }

    #[test]
    fn test_apply_scores_to_tree() {
        let mut tree = make_test_tree();
        let scores: HashMap<u32, f32> = [(2, 0.95), (3, 0.88), (5, 0.1)].into_iter().collect();

        apply_scores_to_tree(&mut tree, &scores);

        // Kontrollera att scores applicerats
        assert!(
            tree[0].children[0].relevance > 0.9,
            "Nod 2 borde ha score ~0.95"
        );
        assert!(
            tree[0].children[1].relevance > 0.8,
            "Nod 3 borde ha score ~0.88"
        );
    }
}
