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

/// Sigmoid-normalisering: bevarar absolut signal istället för min-max.
///
/// Min-max förstör information: om alla survivors är lika dåliga sprider den
/// dem artificiellt till [0, 1]. Sigmoid centrerar kring median och separerar
/// genuint bättre noder utan att förstöra absolutskalan.
#[cfg(any(feature = "colbert", test))]
fn normalize_scores(scores: &[f32]) -> Vec<f32> {
    if scores.is_empty() {
        return vec![];
    }
    if scores.len() == 1 {
        return vec![0.5];
    }

    // Median och IQR för robust centrering
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    let q1 = sorted[sorted.len() / 4];
    let q3 = sorted[sorted.len() * 3 / 4];
    let iqr = q3 - q1;
    let scale = if iqr > 1e-9 { iqr } else { 1.0 };

    scores
        .iter()
        .map(|&s| {
            let z = (s - median) / scale;
            1.0 / (1.0 + (-z * 2.0).exp()) // sigmoid med lite skärpa
        })
        .collect()
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
    // Normalisera per query-token: avg similarity istället för summa.
    // Förhindrar att långa noder med fler matchnings-targets dominerar.
    total as f32 / q.num_tokens as f32
}

// ── Nod-filtrering och scoring-justeringar ───────────────────────────────────

/// Avgör om en nod ska filtreras bort helt (score=0).
/// Matchar logiken i embed_score.rs is_template_artifact + is_numeric_artifact.
#[cfg(feature = "colbert")]
fn should_filter_node(label: &str, role: &str) -> bool {
    if label.is_empty() {
        return true;
    }

    // M2b: i18n-template artefakter — hela commonI18nResources namespace
    let lower = label.to_ascii_lowercase();
    if lower.starts_with("commoni18nresources.")
        || lower.starts_with("translations.")
        || lower.starts_with("i18n.")
        || lower.starts_with("locale.")
        || lower.starts_with("messages.")
        || lower.starts_with("fields.sections[") // Bugg 5: xe.com converter fields
        || lower.starts_with("dynamicids[") // Bugg 2: Louvre dynamicIds
        || label.contains("{{")
    {
        return true;
    }

    // L3: jsonLd array image/url/src — bildlänkar och metadata
    if (lower.starts_with("jsonld.") || lower.starts_with("props.initialstate."))
        && (lower.contains(".image")
            || lower.contains(".url:")
            || lower.contains(".src")
            || lower.contains(".href")
            || lower.contains(".width")
            || lower.contains(".height")
            || lower.contains(".@type")
            || lower.contains(".@context")
            || lower.contains(".robots")
            || lower.contains(".canonical")
            || lower.contains(".og.")
            || lower.contains(".twitter."))
    {
        return true;
    }

    // Bugg 8: Generellt array-index media-filter
    // jsonLd.recipeInstructions[N].image, review[N].url, etc.
    if lower.contains("].image")
        || lower.contains("].photo")
        || lower.contains("].thumbnail")
        || (lower.contains("].url") && role == "data" && lower.contains("://"))
    {
        return true;
    }

    // Fix 5: OpenGraph/Twitter metadata URLs oavsett prefix
    if lower.contains("og:image")
        || lower.contains("og:url")
        || lower.contains("twitter:image")
        || lower.starts_with("opengraph.")
    {
        return true;
    }
    // Data-noder med bild-URLer
    if role == "data"
        && lower.contains("://")
        && (lower.contains(".png")
            || lower.contains(".jpg")
            || lower.contains(".svg")
            || lower.contains(".webp"))
    {
        return true;
    }

    // JS-twin: data-noder med props.initialState-prefix (louvre.fr etc.)
    if role == "data" && lower.starts_with("props.") {
        return true;
    }

    // Dot-path programmatiska nycklar (samma som embed_score.rs)
    if !label.contains(' ')
        && label.matches('.').count() >= 2
        && label.len() < 200
        && !label.contains("://")
        && !label.starts_with(|c: char| c.is_ascii_digit())
    {
        let has_camel = label.chars().any(|c| c.is_ascii_uppercase());
        let has_bracket = label.contains('[');
        if has_camel || has_bracket {
            return true;
        }
    }

    // Numeriska artefakter (badge-räknare, etc.)
    let trimmed = label.trim();
    if !trimmed.is_empty()
        && trimmed.len() <= 20
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        return true;
    }

    false
}

/// Avgör om label ser ut som en nyhetsrubrik (title-case, inga siffror/valuta).
/// Nyhetsrubriker i sidofält matchar ekonomitermer men är inte faktasvar.
#[cfg(feature = "colbert")]
fn is_news_headline(label: &str) -> bool {
    // Krav: >20 chars, inga siffror, inga %/$£€, title-case
    if label.len() < 20 || label.len() > 200 {
        return false;
    }
    let has_digits = label.chars().any(|c| c.is_ascii_digit());
    let has_currency =
        label.contains('%') || label.contains('$') || label.contains('£') || label.contains('€');
    if has_digits || has_currency {
        return false; // Siffror/valuta = trolig data, inte rubrik
    }
    // Title-case: >50% av orden börjar med versal
    let words: Vec<&str> = label.split_whitespace().collect();
    if words.len() < 4 {
        return false;
    }
    let uppercase_words = words
        .iter()
        .filter(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
        .count();
    uppercase_words as f32 / words.len() as f32 > 0.5
}

/// Detektera nav-listor: korta versala ord utan meningsstruktur.
/// "G Gallium Germanium Gadolinium Gold", "Home About Contact Services"
#[cfg(feature = "colbert")]
fn is_nav_word_list(label: &str) -> bool {
    let words: Vec<&str> = label.split_whitespace().collect();
    if words.len() < 3 || words.len() > 30 {
        return false;
    }
    let short_capitalized = words
        .iter()
        .filter(|w| {
            w.len() <= 15
                && w.chars().next().is_some_and(|c| c.is_uppercase())
                && !w.contains('.')
                && !w.contains(',')
        })
        .count();
    short_capitalized as f32 / words.len() as f32 > 0.7
        && !label.contains(". ")
        && !label.chars().any(|c| c.is_ascii_digit())
}

/// Avgör om label har faktiskt informationsinnehåll (inte nav/boilerplate).
#[cfg(feature = "colbert")]
fn has_informational_content(label: &str) -> bool {
    label.len() > 60
        || label.chars().any(|c| c.is_ascii_digit())
        || label.contains(". ")
        || label.contains(", ")
}

/// Role-baserad score-multiplikator med is_leaf-medvetenhet.
/// Wrapper-noder (is_leaf=false) straffas hårdare — de aggregerar barntext.
#[cfg(feature = "colbert")]
fn role_multiplier(role: &str, label: &str, is_leaf: bool) -> f32 {
    match role {
        // Text-noder — boost BARA om informationsinnehåll (Bugg 1 fix)
        "text" if is_news_headline(label) => 0.4,
        "text" if is_nav_word_list(label) => 0.3, // Nav-listor: "G Gallium Germanium Gold" // Nyhetsrubriker i sidebar
        "text" if label.starts_with("Between ") || label.starts_with("Before ") => 0.85,
        "text" if has_informational_content(label) && label.len() > 50 => 1.15,
        "text" if has_informational_content(label) => 1.05,
        "text" => 0.95,
        // Tabeller = text i boost-hierarkin (Bugg 1 fix: table parity)
        "table" if has_informational_content(label) => 1.15,
        "table" => 1.0,
        // Strukturerad data — boost
        "row" | "cell" | "definition" => 1.10,
        "data" => 1.15,
        // Headings — mild penalty
        "heading" => 0.95,
        // K-nav: step-by-step navigation (GOV.UK)
        "listitem"
            if label.starts_with("Step ")
                || label.contains("step by step")
                || label.contains("You are currently viewing:") =>
        {
            0.3
        }
        "listitem" => 1.10,
        // Links: aldrig boost, alltid penalty (Bugg 1 fix)
        // Links är referenser/navigation, inte svar. Artikeltitlar boostas
        // separat via leaf-link boost (30-200 chars) i scoring-blocket.
        "link" if label.starts_with('[') || label.starts_with('^') || label.starts_with('"') => 0.2,
        "link" if label.len() < 30 => 0.5,
        "link" if label.contains("disambiguation") || label.contains("(page does not exist)") => {
            0.2
        }
        "link" => 0.7, // Neutrala links — aldrig över 0.7
        // Navigation — AGGRESSIV penalty, speciellt wrappers
        "navigation" => {
            if is_leaf {
                0.5
            } else {
                0.20 // Wrapper-nav (aggregerar alla child-links) — kraftig penalty
            }
        }
        // Sidebar
        "complementary" => {
            if is_leaf {
                0.4
            } else {
                0.20
            }
        }
        // Generic wrappers — penalty om inte löv
        "generic" if !is_leaf && label.len() > 100 => 0.6,
        "main" | "banner" if !is_leaf => 0.7,
        _ => 1.0,
    }
}

/// Wrapper-penalty: strukturella noder med aggregerad barntext straffas.
/// Port av embed_score.rs logik (rad 193-209).
#[cfg(feature = "colbert")]
fn wrapper_penalty(role: &str, label_len: usize) -> f32 {
    let is_structural = matches!(
        role,
        "generic" | "table" | "main" | "banner" | "complementary" | "navigation"
    );
    if is_structural && label_len > 200 {
        0.20
    } else if label_len > 500 {
        0.15
    } else if is_structural && label_len > 100 {
        0.10
    } else if label_len > 300 {
        0.08
    } else {
        0.0
    }
}

/// Length-penalty med is_leaf-medvetenhet.
/// Icke-löv-noder (wrappers) straffas hårdare vid kortare längder
/// eftersom deras text är aggregerad från barn.
#[cfg(feature = "colbert")]
fn length_penalty(label_len: usize, is_leaf: bool) -> f32 {
    if is_leaf {
        if label_len > 1000 {
            0.7
        } else if label_len > 500 {
            0.85
        } else {
            1.0
        }
    } else {
        // Icke-löv: aggregerad text, straffas tidigare
        if label_len > 500 {
            0.5
        } else if label_len > 200 {
            0.65
        } else if label_len > 100 {
            0.80
        } else {
            1.0
        }
    }
}

// ── Query expansion ──────────────────────────────────────────────────────────

/// Expandera query med hög-IDF termer från top-BM25-survivors.
///
/// Tar top-5 survivors (sorterade efter BM25-score), extraherar ord,
/// rankar efter IDF (lågfrekventa = informativa), och lägger till max 4
/// expansionstermer som inte redan finns i query.
///
/// Expansion-termer ges lägre vikt i ColBERT via positionen i query-strängen
/// (de hamnar efter [SEP] i tokeniseringen → lägre attention).
#[cfg(feature = "colbert")]
fn expand_query(
    goal: &str,
    survivors: &[(u32, f32)],
    all_nodes: &HashMap<u32, NodeInfo>,
) -> String {
    const MAX_EXPANSION_TERMS: usize = 4;
    const MIN_WORD_LEN: usize = 4; // Skippa korta ord (a, the, is, etc.)

    let goal_lower = goal.to_lowercase();
    let goal_words: std::collections::HashSet<&str> = goal_lower.split_whitespace().collect();

    // Ta top-5 BM25-survivors
    let mut top_survivors: Vec<(u32, f32)> = survivors.to_vec();
    top_survivors.sort_by(|a, b| b.1.total_cmp(&a.1));
    top_survivors.truncate(5);

    // Räkna ordfrekvens i top-survivors
    let mut word_freq: HashMap<String, u32> = HashMap::new();
    let mut total_words = 0u32;
    for &(id, _) in &top_survivors {
        if let Some(info) = all_nodes.get(&id) {
            for word in info.label.to_lowercase().split_whitespace() {
                // Skippa korta ord, siffror, URL-fragment
                if word.len() < MIN_WORD_LEN
                    || word.chars().all(|c| c.is_ascii_digit())
                    || word.contains("://")
                    || word.contains('.')
                {
                    continue;
                }
                *word_freq.entry(word.to_string()).or_insert(0) += 1;
                total_words += 1;
            }
        }
    }

    if word_freq.is_empty() || total_words == 0 {
        return goal.to_string();
    }

    // IDF = log(total / freq) — lågfrekventa ord i survivors = informativa
    let total_f = total_words as f32;
    let mut scored_words: Vec<(String, f32)> = word_freq
        .into_iter()
        .filter(|(word, _)| !goal_words.contains(word.as_str()))
        .map(|(word, freq)| {
            let idf = (total_f / freq as f32).ln();
            (word, idf)
        })
        .collect();
    scored_words.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Ta top expansionstermer
    let expansion: Vec<String> = scored_words
        .into_iter()
        .take(MAX_EXPANSION_TERMS)
        .map(|(w, _)| w)
        .collect();

    if expansion.is_empty() {
        goal.to_string()
    } else {
        format!("{} {}", goal, expansion.join(" "))
    }
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

/// Beräkna ColBERT multi-signal score för en enskild nod.
/// Används av bottom-up scoringen för både löv- och icke-löv-noder.
#[cfg(feature = "colbert")]
fn compute_colbert_node_score(
    id: u32,
    info: &NodeInfo,
    colbert_map: &HashMap<u32, f32>,
    bm25_map: &HashMap<u32, f32>,
    hdc_text_sims: &HashMap<u32, f32>,
) -> f32 {
    let colbert_score = colbert_map.get(&id).copied().unwrap_or(0.0);
    let bm25_score = bm25_map.get(&id).copied().unwrap_or(0.0);
    let bm25_norm = (bm25_score / 3.0).min(1.0);
    let role_score = crate::types::SemanticNode::role_priority(&info.role);
    let hdc_raw = hdc_text_sims.get(&id).copied().unwrap_or(0.0);
    let hdc_text = (hdc_raw + 1.0) / 2.0;
    let w_penalty = wrapper_penalty(&info.role, info.label.len());
    let len_pen = length_penalty(info.label.len(), info.is_leaf);
    let role_mult = role_multiplier(&info.role, &info.label, info.is_leaf);

    let raw = (colbert_score * 0.40 + hdc_text * 0.15 + role_score * 0.15 + bm25_norm * 0.30
        - w_penalty)
        * role_mult
        * len_pen;

    raw.clamp(0.0, 1.0)
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
    hdc_text_sims: &HashMap<u32, f32>,
) -> Vec<ScoredNode> {
    // Kolla cache
    let key = cache_key(goal, survivors, all_nodes);
    let cache = COLBERT_CACHE.get_or_init(|| Mutex::new(ColbertScoreCache::new(64)));
    if let Ok(c) = cache.lock() {
        if let Some(cached) = c.get(key) {
            return cached.clone();
        }
    }

    // Query expansion: lägg till hög-IDF termer från top-BM25-survivors.
    // Bron mellan BM25 (lexical recall) och ColBERT (semantic precision).
    // Ex: goal "population stockholm" + expansion "inhabitants" → bättre recall.
    let expanded_goal = expand_query(goal, survivors, all_nodes);

    let q_embs = match crate::embedding::encode_tokens(&expanded_goal) {
        Some(e) if !e.is_empty() => e,
        _ => return fallback_zero_scores(survivors, all_nodes),
    };

    let (ids, raw_scores) = batch_colbert_scores(&q_embs, survivors, all_nodes);
    let normed = normalize_scores(&raw_scores);

    // Bygg lookups
    let bm25_map: HashMap<u32, f32> = survivors.iter().copied().collect();
    let colbert_map: HashMap<u32, f32> = ids
        .iter()
        .zip(normed.iter())
        .map(|(&id, &s)| (id, s))
        .collect();

    // ── Bottom-up scoring (samma design som embed_score.rs) ──
    // Steg 1: Scorea löv-noder direkt med ColBERT multi-signal
    let mut scores: HashMap<u32, f32> = HashMap::new();
    const PARENT_DECAY: f32 = 0.75;

    for &(node_id, _) in survivors {
        if let Some(info) = all_nodes.get(&node_id) {
            if !info.is_leaf {
                continue; // Icke-löv väntar
            }
            if should_filter_node(&info.label, &info.role) {
                continue;
            }
            let score =
                compute_colbert_node_score(node_id, info, &colbert_map, &bm25_map, hdc_text_sims);
            scores.insert(node_id, score);
        }
    }

    // Steg 2: Icke-löv ärver max(barn) × PARENT_DECAY, men minst egen score
    for &(node_id, _) in survivors {
        if scores.contains_key(&node_id) {
            continue; // Redan scoread (löv)
        }
        if let Some(info) = all_nodes.get(&node_id) {
            if should_filter_node(&info.label, &info.role) {
                continue;
            }
            // Max barn-score
            let max_child = info
                .child_ids
                .iter()
                .filter_map(|cid| scores.get(cid))
                .copied()
                .fold(0.0f32, f32::max);

            // Egen score
            let own_score =
                compute_colbert_node_score(node_id, info, &colbert_map, &bm25_map, hdc_text_sims);

            // Ta max av (barn-arv, egen score)
            let inherited = max_child * PARENT_DECAY;
            scores.insert(node_id, own_score.max(inherited));
        }
    }

    // Steg 3: Samla resultat
    let mut result: Vec<ScoredNode> = survivors
        .iter()
        .filter_map(|&(id, _)| {
            let info = all_nodes.get(&id)?;
            let relevance = scores.get(&id).copied().unwrap_or(0.0);
            if relevance <= 0.0 {
                return None;
            }

            // Leaf-link boost: artikeltitlar, produktnamn (30-200 chars)
            let boosted = if info.is_leaf
                && info.role == "link"
                && info.label.len() >= 30
                && info.label.len() <= 200
            {
                relevance * 1.15
            } else {
                relevance
            };

            Some(ScoredNode {
                id,
                relevance: boosted.clamp(0.0, 1.0),
                role: info.role.clone(),
                label: info.label.clone(),
            })
        })
        .collect();

    result.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));

    // Dedup: identiska/substring/overlap labels → behåll bara högst-scorade
    {
        let mut seen_labels: Vec<String> = Vec::new();
        result.retain(|node| {
            let key: String = node
                .label
                .chars()
                .take(120)
                .collect::<String>()
                .trim()
                .to_string();
            if key.is_empty() || key.len() < 4 {
                return true;
            }
            // Exakt substring match
            let is_substring = seen_labels
                .iter()
                .any(|prev| prev.contains(&key) || key.contains(prev.as_str()));
            if is_substring {
                return false;
            }
            // Ord-overlap: om >70% av orden redan setts i en högre-rankad nod → dup
            // Fångar "Group 11 Melting..." vs "Fact box Group 11 Melting..."
            let key_words: Vec<&str> = key.split_whitespace().collect();
            if key_words.len() >= 4 {
                let is_overlap = seen_labels.iter().any(|prev| {
                    let matches = key_words.iter().filter(|w| prev.contains(**w)).count();
                    matches as f32 / key_words.len() as f32 > 0.7
                });
                if is_overlap {
                    return false;
                }
            }
            seen_labels.push(key);
            true
        });
    }

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

    // Dedup med substring-check (Bugg 4 fix)
    {
        let mut seen_labels: Vec<String> = Vec::new();
        result.retain(|node| {
            let key: String = node
                .label
                .chars()
                .take(120)
                .collect::<String>()
                .trim()
                .to_string();
            if key.is_empty() || key.len() < 4 {
                return true;
            }
            let is_dup = seen_labels
                .iter()
                .any(|prev| prev.contains(&key) || key.contains(prev.as_str()));
            if is_dup {
                return false;
            }
            seen_labels.push(key);
            true
        });
    }

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
        // Sigmoid: min < median < max, men inte exakt 0/1
        assert!(
            normed[0] < normed[2],
            "Min borde vara lägst, fick min={}, mellan={}",
            normed[0],
            normed[2]
        );
        assert!(
            normed[3] > normed[2],
            "Max borde vara högst, fick max={}, mellan={}",
            normed[3],
            normed[2]
        );
        // Alla borde vara i (0, 1) — sigmoid ger aldrig exakt 0 eller 1
        assert!(
            normed.iter().all(|&s| s > 0.0 && s < 1.0),
            "Alla borde vara i (0,1), fick {:?}",
            normed
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
            normed[0] < 0.5,
            "Min borde vara under 0.5, fick {}",
            normed[0]
        );
        assert!(
            normed[1] >= 0.5,
            "Max borde vara >= 0.5, fick {}",
            normed[1]
        );
        assert!(normed[1] > normed[0], "Max borde vara högre än min");
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
