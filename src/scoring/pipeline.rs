use super::hdc::HdcTree;
use super::tfidf::TfIdfIndex;
/// ScoringPipeline — orchestrerar de tre stegen
///
/// 1. TF-IDF kandidatretrieval
/// 2. HDC pruning
/// 3. Embedding bottom-up scoring
use crate::types::SemanticNode;

/// Resultat från hybrid-pipelinen
#[derive(Debug, Clone)]
pub struct ScoredNode {
    pub id: u32,
    pub relevance: f32,
}

pub struct ScoringPipeline;
