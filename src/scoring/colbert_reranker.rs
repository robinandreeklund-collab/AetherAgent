// ColBERTv2 Late Interaction Reranker
//
// Implementerar MaxSim-operatorn (Khattab & Zaharia, SIGIR 2020):
// Varje token i query och dokument behåller sin egen embedding.
// Score = Σ_i max_j cosine(q_i, d_j)
//
// Fördel jämfört med bi-encoder (mean pooling):
// Långa noder med blandad info rankas korrekt — faktanoder med pris/volym/data
// slår rubriker och nav-noder som bara delar nyckelord globalt.

#[cfg(any(feature = "colbert", test))]
use std::collections::HashMap;
#[cfg(feature = "colbert")]
use std::path::Path;
#[cfg(feature = "colbert")]
use std::sync::{Arc, OnceLock};

#[cfg(feature = "colbert")]
use candle_core::{DType, Device, Tensor};
#[cfg(feature = "colbert")]
use candle_nn::VarBuilder;
#[cfg(feature = "colbert")]
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
#[cfg(feature = "colbert")]
use tokenizers::Tokenizer;

#[cfg(any(feature = "colbert", test))]
use super::embed_score::{NodeInfo, ScoredNode};

/// ColBERT max token length per nod
#[cfg(feature = "colbert")]
const COLBERT_MAX_LEN: usize = 512;

/// Längd-adaptivt alpha: välj ColBERT-vikt baserat på nodlängd (i tokens).
///
/// Bi-encoder fungerar bra på korta, fokuserade noder (rubriker, etiketter).
/// ColBERT:s fördel är störst på långa noder med blandad information —
/// tabellrader, footers, nyhetstext.
pub fn adaptive_alpha(node_token_len: usize) -> f32 {
    match node_token_len {
        0..=20 => 0.3,
        21..=80 => 0.7,
        81..=200 => 0.85,
        _ => 0.95,
    }
}

/// Normalisera scores till [0, 1]
#[cfg(any(feature = "colbert", test))]
fn normalize_scores(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return vec![];
    }
    let min = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = max - min;
    if range < 1e-9 {
        return vec![0.5; scores.len()];
    }
    scores.iter().map(|&s| (s - min) / range).collect()
}

// ── Stage3Reranker enum (alltid tillgänglig) ─────────────────────────────────

/// Vilken reranker som används för Stage 3 scoring.
///
/// Default: `MiniLM` (befintlig bi-encoder).
/// ColBERT och Hybrid kräver `colbert` feature flag.
#[derive(Debug, Clone, Default)]
pub enum Stage3Reranker {
    /// Befintlig bi-encoder (all-MiniLM-L6-v2) — default
    #[default]
    MiniLM,
    /// ColBERTv2 MaxSim late interaction
    #[cfg(feature = "colbert")]
    ColBert {
        /// Sökväg till modellkatalog (config.json, tokenizer.json, model.safetensors)
        model_dir: std::path::PathBuf,
    },
    /// Hybrid: alpha × ColBERT + (1 - alpha) × MiniLM
    #[cfg(feature = "colbert")]
    Hybrid {
        /// Sökväg till ColBERT-modell
        model_dir: std::path::PathBuf,
        /// Global alpha-vikt (0.0 = ren MiniLM, 1.0 = ren ColBERT)
        alpha: f32,
        /// Om true: alpha varierar per nod baserat på token-längd
        use_adaptive_alpha: bool,
    },
}

// ── ColBERT scoring (feature-gatad) ──────────────────────────────────────────

/// Score survivors med ColBERT MaxSim.
/// Returnerar `ScoredNode`-lista sorterad efter relevance (högst först).
#[cfg(feature = "colbert")]
pub fn score_colbert(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
    goal: &str,
    model_dir: &Path,
) -> Vec<ScoredNode> {
    let reranker = match get_or_init_colbert(model_dir) {
        Ok(r) => r,
        Err(_) => return fallback_zero_scores(survivors, all_nodes),
    };

    let texts: Vec<&str> = survivors
        .iter()
        .filter_map(|&(id, _)| all_nodes.get(&id).map(|info| info.label.as_str()))
        .collect();

    let ids: Vec<u32> = survivors
        .iter()
        .filter_map(|&(id, _)| {
            if all_nodes.contains_key(&id) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    let scores = reranker
        .score_batch(goal, &texts)
        .unwrap_or_else(|_| vec![0.0; texts.len()]);

    let mut result: Vec<ScoredNode> = ids
        .iter()
        .zip(scores.iter())
        .filter_map(|(&id, &score)| {
            let info = all_nodes.get(&id)?;
            Some(ScoredNode {
                id,
                relevance: score.min(1.0),
                role: info.role.clone(),
                label: info.label.clone(),
            })
        })
        .collect();

    result.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
    result
}

/// Score survivors med hybrid ColBERT + MiniLM.
///
/// `minilm_scores` — pre-computed bi-encoder scores (från `score_bottom_up`)
/// `alpha` — global vikt (0.7 = 70% ColBERT, 30% MiniLM)
/// `use_adaptive_alpha` — om true, alpha varierar per nod baserat på token-längd
#[cfg(feature = "colbert")]
pub fn score_hybrid(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
    goal: &str,
    minilm_scores: &[ScoredNode],
    model_dir: &Path,
    alpha: f32,
    use_adaptive_alpha: bool,
) -> Vec<ScoredNode> {
    let reranker = match get_or_init_colbert(model_dir) {
        Ok(r) => r,
        Err(_) => return minilm_scores.to_vec(),
    };

    // Bygg MiniLM score-map
    let minilm_map: HashMap<u32, f32> = minilm_scores.iter().map(|n| (n.id, n.relevance)).collect();

    let texts: Vec<&str> = survivors
        .iter()
        .filter_map(|&(id, _)| all_nodes.get(&id).map(|info| info.label.as_str()))
        .collect();

    let ids: Vec<u32> = survivors
        .iter()
        .filter_map(|&(id, _)| {
            if all_nodes.contains_key(&id) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    let colbert_scores = reranker
        .score_batch(goal, &texts)
        .unwrap_or_else(|_| vec![0.0; texts.len()]);

    let mut result: Vec<ScoredNode> = ids
        .iter()
        .zip(colbert_scores.iter())
        .filter_map(|(&id, &colbert_score)| {
            let info = all_nodes.get(&id)?;
            let minilm_score = minilm_map.get(&id).copied().unwrap_or(0.0);

            let a = if use_adaptive_alpha {
                let token_len = info.label.split_whitespace().count();
                adaptive_alpha(token_len)
            } else {
                alpha
            };

            let combined = a * colbert_score + (1.0 - a) * minilm_score;

            Some(ScoredNode {
                id,
                relevance: combined.min(1.0),
                role: info.role.clone(),
                label: info.label.clone(),
            })
        })
        .collect();

    result.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
    result
}

/// Fallback: alla noder får score 0 (om modell-laddning misslyckas)
#[cfg(any(feature = "colbert", test))]
fn fallback_zero_scores(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
) -> Vec<ScoredNode> {
    survivors
        .iter()
        .filter_map(|&(id, _)| {
            let info = all_nodes.get(&id)?;
            Some(ScoredNode {
                id,
                relevance: 0.0,
                role: info.role.clone(),
                label: info.label.clone(),
            })
        })
        .collect()
}

// ── ColBertReranker (feature-gatad) ──────────────────────────────────────────

#[cfg(feature = "colbert")]
pub struct ColBertReranker {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

#[cfg(feature = "colbert")]
impl ColBertReranker {
    /// Ladda ColBERTv2 från katalog med config.json, tokenizer.json, model.safetensors
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        let device = Device::Cpu;

        let config_path = model_dir.join("config.json");
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Kunde inte läsa config.json: {e}"))?;
        let config: BertConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Kunde inte tolka config.json: {e}"))?;

        let weights_path = model_dir.join("model.safetensors");
        // SAFETY: memory-mapping av safetensors-fil — standard i Candle
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)
                .map_err(|e| format!("Kunde inte ladda weights: {e}"))?
        };

        let model =
            BertModel::load(vb, &config).map_err(|e| format!("Kunde inte bygga BertModel: {e}"))?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Kunde inte ladda tokenizer: {e}"))?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    /// Encode text → per-token embeddings `[seq_len, hidden_dim]`
    fn encode(&self, text: &str) -> Result<Tensor, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenisering misslyckades: {e}"))?;

        let ids = encoding.get_ids();
        let len = ids.len().min(COLBERT_MAX_LEN);
        let truncated: Vec<u32> = ids[..len].to_vec();

        // Candle kräver Vec<i64> för BERT input
        let ids_i64: Vec<i64> = truncated.iter().map(|&id| id as i64).collect();
        let seq_len = ids_i64.len();

        let input_ids =
            Tensor::from_vec(ids_i64, (1, seq_len), &self.device).map_err(|e| format!("{e}"))?;

        let token_type_ids = input_ids
            .zeros_like()
            .map_err(|e| format!("zeros_like misslyckades: {e}"))?;

        let output = self
            .model
            .forward(&input_ids, &token_type_ids, None)
            .map_err(|e| format!("Forward pass misslyckades: {e}"))?;

        // Ta bort batch-dim: [1, seq_len, dim] → [seq_len, dim]
        output
            .squeeze(0)
            .map_err(|e| format!("Squeeze misslyckades: {e}"))
    }

    /// MaxSim-operatorn (Khattab & Zaharia 2020):
    /// score = Σ_i max_j cosine(q_i, d_j)
    fn maxsim(q: &Tensor, d: &Tensor) -> Result<f32, String> {
        let q_norm = l2_normalize(q)?;
        let d_norm = l2_normalize(d)?;

        // Similarity-matris: [q_len, d_len]
        let d_t = d_norm.t().map_err(|e| format!("Transpose: {e}"))?;
        let sim = q_norm.matmul(&d_t).map_err(|e| format!("Matmul: {e}"))?;

        // Per query-token: max similarity mot doc-tokens
        let max_sims = sim.max(1).map_err(|e| format!("Max: {e}"))?;

        // Summera alla max-similarities
        max_sims
            .sum_all()
            .map_err(|e| format!("Sum: {e}"))?
            .to_scalar::<f32>()
            .map_err(|e| format!("to_scalar: {e}"))
    }

    /// Score en batch nod-texter mot en query.
    /// Returnerar normaliserade scores i [0, 1].
    pub fn score_batch(&self, query: &str, nodes: &[&str]) -> Result<Vec<f32>, String> {
        if nodes.is_empty() {
            return Ok(vec![]);
        }

        let q_embs = self.encode(query)?;
        let mut raw_scores = Vec::with_capacity(nodes.len());

        for node_text in nodes {
            if node_text.is_empty() {
                raw_scores.push(0.0);
                continue;
            }
            let d_embs = self.encode(node_text)?;
            let score = Self::maxsim(&q_embs, &d_embs)?;
            raw_scores.push(score);
        }

        Ok(normalize_scores(&raw_scores))
    }
}

/// L2-normalisera varje rad i en 2D-tensor `[rows, dim]`
#[cfg(feature = "colbert")]
fn l2_normalize(t: &Tensor) -> Result<Tensor, String> {
    let norm = t
        .sqr()
        .map_err(|e| format!("sqr: {e}"))?
        .sum_keepdim(1)
        .map_err(|e| format!("sum_keepdim: {e}"))?
        .sqrt()
        .map_err(|e| format!("sqrt: {e}"))?;

    // Undvik division med noll
    let eps = Tensor::new(&[[1e-12f32]], t.device()).map_err(|e| format!("eps tensor: {e}"))?;
    let safe_norm = norm
        .broadcast_maximum(&eps)
        .map_err(|e| format!("maximum: {e}"))?;

    t.broadcast_div(&safe_norm).map_err(|e| format!("div: {e}"))
}

// ── Global singleton för lazy modell-laddning ────────────────────────────────

#[cfg(feature = "colbert")]
static COLBERT_INSTANCE: OnceLock<Arc<ColBertReranker>> = OnceLock::new();

#[cfg(feature = "colbert")]
fn get_or_init_colbert(model_dir: &Path) -> Result<Arc<ColBertReranker>, String> {
    // OnceLock::get_or_try_init är instabil — manuell fallback
    if let Some(instance) = COLBERT_INSTANCE.get() {
        return Ok(instance.clone());
    }
    let reranker = Arc::new(ColBertReranker::load(model_dir)?);
    // Ignorera race — första tråden vinner, alla får samma resultat
    let _ = COLBERT_INSTANCE.set(reranker.clone());
    Ok(COLBERT_INSTANCE.get().cloned().unwrap_or(reranker))
}

// ── Tester ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_scores_basic() {
        let scores = vec![1.0, 3.0, 2.0, 5.0];
        let normed = normalize_scores(&scores);
        assert!(
            (normed[0] - 0.0).abs() < 1e-6,
            "Min borde vara 0.0, fick {}",
            normed[0]
        );
        assert!(
            (normed[3] - 1.0).abs() < 1e-6,
            "Max borde vara 1.0, fick {}",
            normed[3]
        );
        assert!(
            (normed[2] - 0.25).abs() < 1e-6,
            "Mellannod borde vara 0.25, fick {}",
            normed[2]
        );
    }

    #[test]
    fn test_normalize_scores_empty() {
        let normed = normalize_scores(&[]);
        assert!(normed.is_empty(), "Tom input borde ge tom output");
    }

    #[test]
    fn test_normalize_scores_identical() {
        let scores = vec![5.0, 5.0, 5.0];
        let normed = normalize_scores(&scores);
        assert!(
            normed.iter().all(|&s| (s - 0.5).abs() < 1e-6),
            "Identiska scores borde alla bli 0.5, fick {:?}",
            normed
        );
    }

    #[test]
    fn test_normalize_scores_two_values() {
        let scores = vec![0.0, 10.0];
        let normed = normalize_scores(&scores);
        assert!(
            (normed[0] - 0.0).abs() < 1e-6,
            "Min borde normaliseras till 0.0"
        );
        assert!(
            (normed[1] - 1.0).abs() < 1e-6,
            "Max borde normaliseras till 1.0"
        );
    }

    #[test]
    fn test_adaptive_alpha_short() {
        assert!(
            (adaptive_alpha(10) - 0.3).abs() < 1e-6,
            "Korta noder (10 tokens) borde ha alpha=0.3"
        );
    }

    #[test]
    fn test_adaptive_alpha_medium() {
        assert!(
            (adaptive_alpha(50) - 0.7).abs() < 1e-6,
            "Mellanlånga noder (50 tokens) borde ha alpha=0.7"
        );
    }

    #[test]
    fn test_adaptive_alpha_long() {
        assert!(
            (adaptive_alpha(150) - 0.85).abs() < 1e-6,
            "Långa noder (150 tokens) borde ha alpha=0.85"
        );
    }

    #[test]
    fn test_adaptive_alpha_very_long() {
        assert!(
            (adaptive_alpha(300) - 0.95).abs() < 1e-6,
            "Mycket långa noder (300 tokens) borde ha alpha=0.95"
        );
    }

    #[test]
    fn test_adaptive_alpha_boundary_20() {
        assert!(
            (adaptive_alpha(20) - 0.3).abs() < 1e-6,
            "Gränsvärde 20 borde ge alpha=0.3"
        );
    }

    #[test]
    fn test_adaptive_alpha_boundary_21() {
        assert!(
            (adaptive_alpha(21) - 0.7).abs() < 1e-6,
            "Gränsvärde 21 borde ge alpha=0.7"
        );
    }

    #[test]
    fn test_stage3_default_is_minilm() {
        let reranker = Stage3Reranker::default();
        assert!(
            matches!(reranker, Stage3Reranker::MiniLM),
            "Default reranker borde vara MiniLM"
        );
    }

    #[test]
    fn test_fallback_zero_scores() {
        let mut all_nodes = HashMap::new();
        all_nodes.insert(
            1,
            NodeInfo {
                role: "text".into(),
                label: "Bitcoin price".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 0,
            },
        );
        all_nodes.insert(
            2,
            NodeInfo {
                role: "link".into(),
                label: "Home".into(),
                is_leaf: true,
                child_ids: vec![],
                depth: 0,
            },
        );

        let survivors = vec![(1, 0.5), (2, 0.3)];
        let result = fallback_zero_scores(&survivors, &all_nodes);

        assert_eq!(result.len(), 2, "Borde returnera 2 noder");
        assert!(
            result.iter().all(|n| n.relevance == 0.0),
            "Fallback borde ge score 0.0 för alla noder"
        );
    }
}
