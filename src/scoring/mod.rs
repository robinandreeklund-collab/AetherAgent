pub mod cache;
pub mod colbert_reranker;
pub mod embed_score;
pub mod hdc;
pub mod page_cache;
pub mod pipeline;
// Scoring Module — Hybrid TF-IDF + HDC + Embedding pipeline
//
// Tre-stegs scoring-pipeline:
// 1. TF-IDF kandidatretrieval — snabb keyword-match
// 2. HDC hierarkisk pruning — bitwise subträd-eliminering
// 3. Embedding final scoring — semantisk precision på survivors
//    (MiniLM bi-encoder default, ColBERTv2 MaxSim opt-in)
pub mod tfidf;

pub use colbert_reranker::Stage3Reranker;
pub use hdc::HdcTree;
pub use pipeline::{PipelineConfig, PipelineResult, ScoringPipeline};
pub use tfidf::TfIdfIndex;
