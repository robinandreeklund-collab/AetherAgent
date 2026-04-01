// ColBERTv2 Late Interaction Reranker (ONNX-backend)
//
// Implementerar MaxSim-operatorn (Khattab & Zaharia, SIGIR 2020):
// Varje token i query och dokument behåller sin egen embedding.
// Score = Σ_i max_j cosine(q_i, d_j)
//
// Backend: Återanvänder befintlig ONNX-modell (all-MiniLM-L6-v2) via embedding.rs.
// Inga extra dependencies — delar ort + ndarray med embeddings-feature.
//
// Fördel jämfört med bi-encoder (mean pooling):
// Långa noder med blandad info rankas korrekt — faktanoder med pris/volym/data
// slår rubriker och nav-noder som bara delar nyckelord globalt.

#[cfg(feature = "colbert")]
use std::collections::HashMap;

#[cfg(feature = "colbert")]
use super::embed_score::{NodeInfo, ScoredNode};

// Dessa imports behövs bara i test (utan colbert-feature aktiv)
#[cfg(all(test, not(feature = "colbert")))]
use super::embed_score::{NodeInfo, ScoredNode};
#[cfg(all(test, not(feature = "colbert")))]
use std::collections::HashMap;

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
    /// Befintlig bi-encoder (all-MiniLM-L6-v2, FP32) — default
    #[default]
    MiniLM,
    /// ColBERT MaxSim late interaction (int8, per-token matching)
    #[cfg(feature = "colbert")]
    ColBert,
    /// Hybrid: alpha × ColBERT + (1 - alpha) × MiniLM
    #[cfg(feature = "colbert")]
    Hybrid {
        /// Global alpha-vikt (0.0 = ren MiniLM, 1.0 = ren ColBERT)
        alpha: f32,
        /// Om true: alpha varierar per nod baserat på token-längd
        use_adaptive_alpha: bool,
    },
}

// ── Kvantiserad MaxSim ────────────────────────────────────────────────────────

/// Scalar-kvantiserad token-vektor: f32 [-1,1] → u8 [0,255].
/// 4x mindre minne, bättre cache-utnyttjande, SIMD-vänlig.
#[cfg(feature = "colbert")]
struct QuantizedTokens {
    /// Flat u8-buffer: [num_tokens × dim]
    data: Vec<u8>,
    num_tokens: usize,
    dim: usize,
}

#[cfg(feature = "colbert")]
impl QuantizedTokens {
    /// Kvantisera f32 token-embeddings → u8.
    /// Input är L2-normaliserade → värden i [-1, 1].
    /// Mapping: u8 = round((f32 + 1.0) * 127.5)
    fn from_f32(tokens: &[Vec<f32>]) -> Self {
        if tokens.is_empty() {
            return Self {
                data: vec![],
                num_tokens: 0,
                dim: 0,
            };
        }
        let dim = tokens[0].len();
        let mut data = Vec::with_capacity(tokens.len() * dim);
        for tok in tokens {
            for &v in tok {
                // Clamp till [-1, 1] (borde redan vara det efter L2-norm)
                let clamped = v.clamp(-1.0, 1.0);
                data.push(((clamped + 1.0) * 127.5) as u8);
            }
        }
        Self {
            data,
            num_tokens: tokens.len(),
            dim,
        }
    }

    /// Hämta token i som u8-slice
    #[inline]
    fn token(&self, i: usize) -> &[u8] {
        let start = i * self.dim;
        &self.data[start..start + self.dim]
    }
}

/// Kvantiserad dot product: u8 × u8 → approximerad cosine similarity.
/// Varje u8 representerar (f32 + 1.0) * 127.5.
/// dot(a,b) ≈ Σ(a_i * b_i) / 127.5² - 1.0 (men vi skippar denormalisering
/// eftersom vi bara behöver relativ ranking, inte absoluta värden).
#[cfg(feature = "colbert")]
#[inline]
fn quantized_dot(a: &[u8], b: &[u8]) -> u32 {
    // u8×u8 summerat till u32 — overflow-säkert upp till dim=65535
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| x as u32 * y as u32)
        .sum()
}

/// MaxSim med kvantiserade vektorer (Khattab & Zaharia 2020):
/// score = Σ_i max_j dot_q(q_i, d_j)
///
/// Använder u8-kvantisering för 4x mindre minnesavtryck och bättre
/// CPU cache-utnyttjande. Relativ ranking bevaras.
#[cfg(feature = "colbert")]
fn maxsim_quantized(q: &QuantizedTokens, d: &QuantizedTokens) -> f32 {
    if q.num_tokens == 0 || d.num_tokens == 0 {
        return 0.0;
    }
    let mut total = 0u64;
    for qi in 0..q.num_tokens {
        let q_tok = q.token(qi);
        let mut best = 0u32;
        for di in 0..d.num_tokens {
            let score = quantized_dot(q_tok, d.token(di));
            if score > best {
                best = score;
            }
        }
        total += best as u64;
    }
    total as f32
}

// ── ColBERT score cache ──────────────────────────────────────────────────────

/// Cache för MaxSim-scores: (goal_hash, survivors_hash) → ScoredNode-lista.
/// Undviker att köra ONNX-inference + MaxSim om samma goal+innehåll queryats.
#[cfg(feature = "colbert")]
use std::sync::Mutex;

#[cfg(feature = "colbert")]
static COLBERT_CACHE: std::sync::OnceLock<Mutex<ColbertScoreCache>> = std::sync::OnceLock::new();

#[cfg(feature = "colbert")]
struct ColbertScoreCache {
    entries: Vec<(u64, Vec<ScoredNode>)>, // (key_hash, scores)
    max_entries: usize,
}

#[cfg(feature = "colbert")]
impl ColbertScoreCache {
    fn new(max: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max),
            max_entries: max,
        }
    }

    fn get(&self, key: u64) -> Option<&Vec<ScoredNode>> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    fn insert(&mut self, key: u64, scores: Vec<ScoredNode>) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0); // FIFO eviction
        }
        self.entries.push((key, scores));
    }
}

/// Beräkna cache-nyckel från goal + survivor-labels
#[cfg(feature = "colbert")]
fn cache_key(goal: &str, survivors: &[(u32, f32)], all_nodes: &HashMap<u32, NodeInfo>) -> u64 {
    // FNV-1a hash av goal + sorterade survivor-IDs
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in goal.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for &(id, _) in survivors {
        hash ^= id as u64;
        hash = hash.wrapping_mul(0x100000001b3);
        // Inkludera label-längd för att skilja på olika sidor med samma IDs
        if let Some(info) = all_nodes.get(&id) {
            hash ^= info.label.len() as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

// ── ColBERT scoring (feature-gatad) ──────────────────────────────────────────

/// Batch-encode alla survivors och beräkna kvantiserad MaxSim mot query.
/// EN ONNX-inference + u8-kvantisering + MaxSim.
#[cfg(feature = "colbert")]
fn batch_colbert_scores(
    q_embs: &[Vec<f32>],
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
) -> (Vec<u32>, Vec<f32>) {
    let mut ids = Vec::with_capacity(survivors.len());
    let mut texts: Vec<&str> = Vec::with_capacity(survivors.len());
    let mut empty_indices: Vec<usize> = Vec::new();

    for &(id, _) in survivors {
        if let Some(info) = all_nodes.get(&id) {
            let idx = ids.len();
            ids.push(id);
            if info.label.is_empty() {
                texts.push("");
                empty_indices.push(idx);
            } else {
                texts.push(&info.label);
            }
        }
    }

    // EN enda ONNX-inference för alla nod-texter
    let batch_embs =
        crate::embedding::encode_tokens_batch(&texts).unwrap_or_else(|| vec![vec![]; texts.len()]);

    // Kvantisera query-tokens en gång (f32 → u8)
    let q_quant = QuantizedTokens::from_f32(q_embs);

    // Kvantiserad MaxSim per nod
    let mut raw_scores = Vec::with_capacity(ids.len());
    for (i, d_embs) in batch_embs.iter().enumerate() {
        if empty_indices.contains(&i) || d_embs.is_empty() {
            raw_scores.push(0.0);
        } else {
            let d_quant = QuantizedTokens::from_f32(d_embs);
            raw_scores.push(maxsim_quantized(&q_quant, &d_quant));
        }
    }

    (ids, raw_scores)
}

/// Score survivors med ColBERT MaxSim via batch ONNX-inference.
/// Cachar resultat per goal+survivors — cache hit skippar ONNX helt.
#[cfg(feature = "colbert")]
pub fn score_colbert(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
    goal: &str,
) -> Vec<ScoredNode> {
    // Kolla cache
    let key = cache_key(goal, survivors, all_nodes);
    let cache = COLBERT_CACHE.get_or_init(|| Mutex::new(ColbertScoreCache::new(64)));
    if let Ok(c) = cache.lock() {
        if let Some(cached) = c.get(key) {
            return cached.clone();
        }
    }

    let q_embs = match crate::embedding::encode_tokens(goal) {
        Some(e) if !e.is_empty() => e,
        _ => return fallback_zero_scores(survivors, all_nodes),
    };

    let (ids, raw_scores) = batch_colbert_scores(&q_embs, survivors, all_nodes);
    let normed = normalize_scores(&raw_scores);

    let mut result: Vec<ScoredNode> = ids
        .iter()
        .zip(normed.iter())
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

    // Spara i cache
    if let Ok(mut c) = cache.lock() {
        c.insert(key, result.clone());
    }

    result
}

/// Score survivors med hybrid ColBERT + MiniLM via batch ONNX-inference.
#[cfg(feature = "colbert")]
pub fn score_hybrid(
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
    goal: &str,
    minilm_scores: &[ScoredNode],
    alpha: f32,
    use_adaptive_alpha: bool,
) -> Vec<ScoredNode> {
    let q_embs = match crate::embedding::encode_tokens(goal) {
        Some(e) if !e.is_empty() => e,
        _ => return minilm_scores.to_vec(),
    };

    let minilm_map: HashMap<u32, f32> = minilm_scores.iter().map(|n| (n.id, n.relevance)).collect();

    let (ids, raw_scores) = batch_colbert_scores(&q_embs, survivors, all_nodes);
    let normed_colbert = normalize_scores(&raw_scores);

    let mut result: Vec<ScoredNode> = ids
        .iter()
        .zip(normed_colbert.iter())
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
