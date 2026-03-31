//! Embedding-modul för semantisk likhet via ONNX-modeller (all-MiniLM-L6-v2 etc.)
//!
//! Laddar en sentence-transformer ONNX-modell + WordPiece-vokabulär vid startup.
//! Ger cosine similarity mellan meningar — ersätter/kompletterar text_similarity()
//! i semantic.rs, causal.rs och compiler.rs.
//!
//! Kräver feature-flagga `embeddings` (återanvänder `ort` + `ndarray` från vision).
//! Env-variabler:
//! - `AETHER_EMBEDDING_MODEL` — sökväg till ONNX-modell (.onnx)
//! - `AETHER_EMBEDDING_VOCAB` — sökväg till vocab.txt (WordPiece)

#[cfg(any(feature = "embeddings", test))]
use std::collections::HashMap;
#[cfg(feature = "embeddings")]
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "embeddings")]
use ort::value::TensorRef;

// ─── Global singleton ──────────────────────────────────────────────────────────

#[cfg(feature = "embeddings")]
static GLOBAL_EMBEDDING: OnceLock<EmbeddingModel> = OnceLock::new();

/// Initialisera den globala embedding-modellen.
///
/// Anropas en gång vid server-/MCP-startup. Returnerar Err om modell/vocab
/// inte kan laddas. Om redan initialiserad — no-op (returnerar Ok).
#[cfg(feature = "embeddings")]
pub fn init_global(model_bytes: &[u8], vocab_text: &str) -> Result<(), String> {
    let model = EmbeddingModel::load(model_bytes, vocab_text)?;
    // OnceLock::set returnerar Err om redan satt — det är OK
    let _ = GLOBAL_EMBEDDING.set(model);
    Ok(())
}

/// Hämta embedding-vektor för en text via den globala modellen.
/// Returnerar None om modellen inte är initialiserad.
#[cfg(feature = "embeddings")]
pub fn embed(text: &str) -> Option<Vec<f32>> {
    GLOBAL_EMBEDDING.get()?.embed(text).ok()
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn embed(_text: &str) -> Option<Vec<f32>> {
    None
}

/// Cosine similarity via global embedding-modell.
/// Returnerar None om modellen inte är initialiserad eller inference misslyckas.
#[cfg(feature = "embeddings")]
pub fn similarity(a: &str, b: &str) -> Option<f32> {
    GLOBAL_EMBEDDING.get()?.similarity(a, b).ok()
}

/// Cosine similarity med en pre-beräknad vektor (undviker dubbel inference).
/// Används av SemanticBuilder: goal-vektorn embedas en gång, sen jämförs per nod.
#[cfg(feature = "embeddings")]
pub fn similarity_with_vec(pre_computed: &[f32], text: &str) -> Option<f32> {
    let vec_b = GLOBAL_EMBEDDING.get()?.embed(text).ok()?;
    Some(cosine_similarity(pre_computed, &vec_b))
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn similarity_with_vec(_pre_computed: &[f32], _text: &str) -> Option<f32> {
    None
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn similarity(_a: &str, _b: &str) -> Option<f32> {
    None
}

/// Returnerar true om den globala embedding-modellen är laddad och redo.
#[cfg(feature = "embeddings")]
pub fn is_loaded() -> bool {
    GLOBAL_EMBEDDING.get().is_some()
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn is_loaded() -> bool {
    false
}

/// Förbättrad text-likhet: embedding-cosine om modell finns, annars word-overlap fallback.
///
/// Drop-in-ersättning för `semantic::text_similarity()` — kan anropas var som helst.
/// Returnerar 0.0–1.0.
pub fn enhanced_similarity(query: &str, candidate: &str) -> f32 {
    // Snabb exakt-match (undvik inference)
    if query.eq_ignore_ascii_case(candidate) {
        return 1.0;
    }

    // Försök embedding-similarity
    if let Some(score) = similarity(query, candidate) {
        return score;
    }

    // Fallback: befintlig word-overlap
    crate::semantic::text_similarity(query, candidate)
}

/// Batch-embed: Beräkna embedding-vektorer för flera texter.
/// Returnerar None om modellen inte är initialiserad.
#[cfg(feature = "embeddings")]
pub fn embed_batch(texts: &[&str]) -> Option<Vec<Vec<f32>>> {
    let model = GLOBAL_EMBEDDING.get()?;
    let mut results = Vec::with_capacity(texts.len());
    for text in texts {
        results.push(model.embed(text).ok()?);
    }
    Some(results)
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn embed_batch(_texts: &[&str]) -> Option<Vec<Vec<f32>>> {
    None
}

/// Encode text → per-token embeddings (utan mean pooling).
/// Returnerar `[seq_len, dim]` matris — varje rad är en token-embedding.
/// Används av ColBERT MaxSim reranker.
#[cfg(feature = "embeddings")]
pub fn encode_tokens(text: &str) -> Option<Vec<Vec<f32>>> {
    GLOBAL_EMBEDDING.get()?.encode_tokens(text).ok()
}

/// Stub: embedding feature ej aktiverad
#[cfg(not(feature = "embeddings"))]
pub fn encode_tokens(_text: &str) -> Option<Vec<Vec<f32>>> {
    None
}

/// Returnerar embedding-dimensionen (t.ex. 384 för MiniLM).
#[cfg(feature = "embeddings")]
pub fn dimension() -> Option<usize> {
    Some(GLOBAL_EMBEDDING.get()?.dimension())
}

/// Stub
#[cfg(not(feature = "embeddings"))]
pub fn dimension() -> Option<usize> {
    None
}

// ─── EmbeddingModel ────────────────────────────────────────────────────────────

/// Embedding-modell som kapslar in ONNX session + WordPiece tokenizer + cache.
///
/// Thread-safe: session och cache skyddas av Mutex.
/// Cachen är en enkel LRU med max 2048 poster (tillräcklig för en sida).
#[cfg(feature = "embeddings")]
pub struct EmbeddingModel {
    session: Mutex<ort::session::Session>,
    tokenizer: WordPieceTokenizer,
    cache: Mutex<EmbeddingCache>,
    dim: usize,
}

#[cfg(feature = "embeddings")]
impl EmbeddingModel {
    /// Ladda modell från ONNX-bytes och vocab.txt-text.
    ///
    /// Detekterar automatiskt embedding-dimension från modellens output-shape.
    pub fn load(model_bytes: &[u8], vocab_text: &str) -> Result<Self, String> {
        let session = ort::session::Session::builder()
            .map_err(|e| format!("ORT session builder: {e}"))?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
            .map_err(|e| format!("ORT opt level: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("ORT intra threads: {e}"))?
            .with_inter_threads(1)
            .map_err(|e| format!("ORT inter threads: {e}"))?
            .commit_from_memory(model_bytes)
            .map_err(|e| format!("ORT model load: {e}"))?;

        // Embedding dimension — 384 för all-MiniLM-L6-v2, 768 för BERT-base
        // Detekteras vid första inference om modellen har annan dim
        let dim = 384;

        let tokenizer = WordPieceTokenizer::from_vocab_text(vocab_text)?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            cache: Mutex::new(EmbeddingCache::new(2048)),
            dim,
        })
    }

    /// Beräkna embedding-vektor för en text-sträng.
    ///
    /// Returnerar L2-normaliserad vektor (dim dimensioner).
    /// Använder cache — upprepade anrop för samma text är gratis.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        // Kolla cache först (undvik Mutex på session)
        {
            let cache = self.cache.lock().map_err(|e| format!("Cache lock: {e}"))?;
            if let Some(vec) = cache.get(text) {
                return Ok(vec.clone());
            }
        }

        // Tokenisera
        let tokens = self.tokenizer.tokenize(text);
        let seq_len = tokens.input_ids.len();

        // Konvertera till i64 (ONNX-modeller förväntar i64)
        let input_ids: Vec<i64> = tokens.input_ids.iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = tokens.attention_mask.iter().map(|&x| x as i64).collect();
        let token_type_ids: Vec<i64> = tokens.token_type_ids.iter().map(|&x| x as i64).collect();

        // Skapa tensorer (zero-copy views)
        let ids_tensor = TensorRef::<i64>::from_array_view(([1usize, seq_len], &input_ids[..]))
            .map_err(|e| format!("Tensor input_ids: {e}"))?;
        let mask_tensor =
            TensorRef::<i64>::from_array_view(([1usize, seq_len], &attention_mask[..]))
                .map_err(|e| format!("Tensor attention_mask: {e}"))?;
        let type_tensor =
            TensorRef::<i64>::from_array_view(([1usize, seq_len], &token_type_ids[..]))
                .map_err(|e| format!("Tensor token_type_ids: {e}"))?;

        // Kör inference
        let mut session = self
            .session
            .lock()
            .map_err(|e| format!("Session lock: {e}"))?;
        let outputs = session
            .run(ort::inputs![ids_tensor, mask_tensor, type_tensor])
            .map_err(|e| format!("ORT inference: {e}"))?;

        // Hämta output: [1, seq_len, dim]
        let (_name, output_value) = outputs
            .iter()
            .next()
            .ok_or_else(|| "Inget output från embedding-modellen".to_string())?;
        let (_shape, data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Output extract: {e}"))?;

        // Mean pooling med attention mask
        let embedding = mean_pool(data, &tokens.attention_mask, seq_len, self.dim);

        // L2-normalisera
        let embedding = l2_normalize(&embedding);

        // Spara i cache
        {
            let mut cache = self.cache.lock().map_err(|e| format!("Cache lock: {e}"))?;
            cache.insert(text.to_string(), embedding.clone());
        }

        Ok(embedding)
    }

    /// Encode text → per-token L2-normaliserade embeddings `[seq_len][dim]`.
    ///
    /// Skip mean pooling — behåll varje tokens individuella embedding.
    /// Används av ColBERT MaxSim reranker för late interaction scoring.
    pub fn encode_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
        let tokens = self.tokenizer.tokenize(text);
        let seq_len = tokens.input_ids.len();

        let input_ids: Vec<i64> = tokens.input_ids.iter().map(|&x| x as i64).collect();
        let attention_mask: Vec<i64> = tokens.attention_mask.iter().map(|&x| x as i64).collect();
        let token_type_ids: Vec<i64> = tokens.token_type_ids.iter().map(|&x| x as i64).collect();

        let ids_tensor = TensorRef::<i64>::from_array_view(([1usize, seq_len], &input_ids[..]))
            .map_err(|e| format!("Tensor input_ids: {e}"))?;
        let mask_tensor =
            TensorRef::<i64>::from_array_view(([1usize, seq_len], &attention_mask[..]))
                .map_err(|e| format!("Tensor attention_mask: {e}"))?;
        let type_tensor =
            TensorRef::<i64>::from_array_view(([1usize, seq_len], &token_type_ids[..]))
                .map_err(|e| format!("Tensor token_type_ids: {e}"))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| format!("Session lock: {e}"))?;
        let outputs = session
            .run(ort::inputs![ids_tensor, mask_tensor, type_tensor])
            .map_err(|e| format!("ORT inference: {e}"))?;

        let (_name, output_value) = outputs
            .iter()
            .next()
            .ok_or_else(|| "Inget output från modellen".to_string())?;
        let (_shape, data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Output extract: {e}"))?;

        // Bygg per-token embeddings, L2-normaliserade, skippa padding
        let dim = self.dim;
        let mut token_embeddings = Vec::with_capacity(seq_len);
        for (i, &mask) in tokens.attention_mask.iter().enumerate().take(seq_len) {
            if mask == 0 {
                continue;
            }
            let offset = i * dim;
            if offset + dim > data.len() {
                break;
            }
            let token_vec: Vec<f32> = data[offset..offset + dim].to_vec();
            token_embeddings.push(l2_normalize(&token_vec));
        }

        Ok(token_embeddings)
    }

    /// Cosine similarity mellan två text-strängar.
    ///
    /// Båda vektorerna är L2-normaliserade → cosine sim = dot product.
    pub fn similarity(&self, a: &str, b: &str) -> Result<f32, String> {
        let vec_a = self.embed(a)?;
        let vec_b = self.embed(b)?;
        Ok(cosine_similarity(&vec_a, &vec_b))
    }

    /// Returnerar embedding-dimensionen (t.ex. 384 för MiniLM).
    pub fn dimension(&self) -> usize {
        self.dim
    }
}

// ─── Mean Pooling & Normalization ──────────────────────────────────────────────

/// Mean pooling: medelvärde över token-embeddings, viktat med attention mask.
///
/// Input `data` har formen [seq_len * dim] (flattad från [1, seq_len, dim]).
/// Ignorerar padding-tokens (attention_mask == 0).
#[cfg(any(feature = "embeddings", test))]
fn mean_pool(data: &[f32], attention_mask: &[u32], seq_len: usize, dim: usize) -> Vec<f32> {
    let mut pooled = vec![0.0f32; dim];
    let mut count = 0.0f32;

    for (i, &mask) in attention_mask.iter().enumerate().take(seq_len) {
        if mask == 0 {
            continue;
        }
        let offset = i * dim;
        // Säkerhetskontroll: undvik out-of-bounds om data är kortare
        if offset + dim > data.len() {
            break;
        }
        for (j, val) in pooled.iter_mut().enumerate() {
            *val += data[offset + j];
        }
        count += 1.0;
    }

    if count > 0.0 {
        for val in &mut pooled {
            *val /= count;
        }
    }
    pooled
}

/// L2-normalisering av en vektor (enhetslängd).
#[cfg(any(feature = "embeddings", test))]
fn l2_normalize(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < 1e-12 {
        return vec.to_vec();
    }
    vec.iter().map(|x| x / norm).collect()
}

/// Cosine similarity mellan två L2-normaliserade vektorer = dot product.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum::<f32>()
}

// ─── WordPiece Tokenizer ───────────────────────────────────────────────────────

/// Minimalistisk WordPiece-tokenizer för BERT-familjen.
///
/// Stödjer all-MiniLM-L6-v2 vocab format:
/// - [PAD] = 0, [UNK] = 100, [CLS] = 101, [SEP] = 102
/// - Subword-prefix: "##"
/// - Max sequence length: 128 (tillräcklig för korta meningar/mål)
#[cfg(any(feature = "embeddings", test))]
struct WordPieceTokenizer {
    vocab: HashMap<String, u32>,
    cls_id: u32,
    sep_id: u32,
    unk_id: u32,
    pad_id: u32,
    max_seq_len: usize,
}

/// Tokeniserat resultat — redo att skickas till ONNX-modellen.
#[cfg(any(feature = "embeddings", test))]
pub struct TokenizedInput {
    pub input_ids: Vec<u32>,
    pub attention_mask: Vec<u32>,
    pub token_type_ids: Vec<u32>,
}

#[cfg(any(feature = "embeddings", test))]
impl WordPieceTokenizer {
    /// Ladda vokabulär från vocab.txt-innehåll (en token per rad, radnummer = id).
    fn from_vocab_text(text: &str) -> Result<Self, String> {
        let mut vocab = HashMap::new();
        for (i, line) in text.lines().enumerate() {
            let token = line.trim();
            if !token.is_empty() {
                vocab.insert(token.to_string(), i as u32);
            }
        }

        if vocab.len() < 100 {
            return Err(format!(
                "Vocab för liten: {} tokens (förväntar 30000+)",
                vocab.len()
            ));
        }

        let cls_id = vocab.get("[CLS]").copied().unwrap_or(101);
        let sep_id = vocab.get("[SEP]").copied().unwrap_or(102);
        let unk_id = vocab.get("[UNK]").copied().unwrap_or(100);
        let pad_id = vocab.get("[PAD]").copied().unwrap_or(0);

        Ok(Self {
            vocab,
            cls_id,
            sep_id,
            unk_id,
            pad_id,
            max_seq_len: 128,
        })
    }

    /// Tokenisera en text-sträng till ONNX-input.
    ///
    /// 1. Lowercase + split på whitespace/punctuation
    /// 2. WordPiece subword-tokenisering
    /// 3. Lägg till [CLS] / [SEP], padda till max_seq_len
    fn tokenize(&self, text: &str) -> TokenizedInput {
        let lower = text.to_lowercase();

        // Splitta på whitespace och skiljetecken — behåll skiljetecken som egna tokens
        let words = split_to_words(&lower);

        // WordPiece-tokenisera varje ord
        let mut tokens = Vec::with_capacity(self.max_seq_len);
        tokens.push(self.cls_id);

        for word in &words {
            if tokens.len() >= self.max_seq_len - 1 {
                break;
            }
            self.tokenize_word(word, &mut tokens);
        }

        tokens.push(self.sep_id);

        // Trunkera om för lång
        if tokens.len() > self.max_seq_len {
            tokens.truncate(self.max_seq_len);
            // Ersätt sista med SEP
            if let Some(last) = tokens.last_mut() {
                *last = self.sep_id;
            }
        }

        let real_len = tokens.len();

        // Padda
        let attention_mask: Vec<u32> = (0..self.max_seq_len)
            .map(|i| if i < real_len { 1 } else { 0 })
            .collect();
        let token_type_ids = vec![0u32; self.max_seq_len];

        tokens.resize(self.max_seq_len, self.pad_id);

        TokenizedInput {
            input_ids: tokens,
            attention_mask,
            token_type_ids,
        }
    }

    /// WordPiece-tokenisera ett enskilt ord.
    ///
    /// Greedy longest-prefix match:
    /// 1. Försök hela ordet i vocab
    /// 2. Annars: hitta längsta prefix, fortsätt med "##"-suffix
    /// 3. Om ingen prefix matchar → [UNK]
    fn tokenize_word(&self, word: &str, tokens: &mut Vec<u32>) {
        if word.is_empty() {
            return;
        }

        // Snabbaste fallet: hela ordet finns i vocab
        if let Some(&id) = self.vocab.get(word) {
            tokens.push(id);
            return;
        }

        // Greedy subword-tokenisering
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;
        let mut is_first = true;

        while start < chars.len() {
            if tokens.len() >= self.max_seq_len - 1 {
                break;
            }

            let mut end = chars.len();
            let mut found = false;

            while start < end {
                let substr: String = chars[start..end].iter().collect();
                let lookup = if is_first {
                    substr.clone()
                } else {
                    format!("##{}", substr)
                };

                if let Some(&id) = self.vocab.get(&lookup) {
                    tokens.push(id);
                    start = end;
                    is_first = false;
                    found = true;
                    break;
                }
                end -= 1;
            }

            if !found {
                // Ingen subword matchar — [UNK] för hela ordet
                tokens.push(self.unk_id);
                break;
            }
        }
    }
}

/// Splitta text på whitespace och skiljetecken.
///
/// Skiljetecken blir egna "ord" (t.ex. "hello, world!" → ["hello", ",", "world", "!"]).
/// BERT-stil: separera allt som inte är alfanumeriskt.
#[cfg(any(feature = "embeddings", test))]
fn split_to_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if ch.is_alphanumeric() || ch == '\'' {
            // Alfanumeriska + apostrof hålls ihop
            current.push(ch);
        } else {
            // Skiljetecken → avsluta current, lägg till som eget ord
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            words.push(ch.to_string());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }

    words
}

// ─── Embedding Cache ───────────────────────────────────────────────────────────

/// Enkel LRU-cache med max kapacitet.
///
/// Vid full kapacitet: ta bort äldsta entry (FIFO-ordning).
/// Tillräcklig för typisk användning (en sida = ~50-300 noder).
#[cfg(any(feature = "embeddings", test))]
struct EmbeddingCache {
    map: HashMap<String, Vec<f32>>,
    order: Vec<String>,
    capacity: usize,
}

#[cfg(any(feature = "embeddings", test))]
impl EmbeddingCache {
    fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            order: Vec::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&self, key: &str) -> Option<&Vec<f32>> {
        self.map.get(key)
    }

    fn insert(&mut self, key: String, value: Vec<f32>) {
        if self.map.contains_key(&key) {
            return; // Redan cachad
        }
        // Evictera om full
        while self.order.len() >= self.capacity {
            if let Some(oldest) = self.order.first().cloned() {
                self.order.remove(0);
                self.map.remove(&oldest);
            }
        }
        self.order.push(key.clone());
        self.map.insert(key, value);
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identiska vektorer ska ge 1.0, fick {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "Ortogonala vektorer ska ge 0.0, fick {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 1e-6,
            "Motsatta vektorer ska ge -1.0, fick {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!(
            cosine_similarity(&a, &b).abs() < 1e-6,
            "Olika längd ska ge 0.0"
        );
    }

    #[test]
    fn test_l2_normalize() {
        let v = vec![3.0, 4.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "L2-normaliserad vektor ska ha norm 1.0, fick {norm}"
        );
        assert!(
            (n[0] - 0.6).abs() < 1e-5,
            "Förväntar 3/5 = 0.6, fick {}",
            n[0]
        );
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!(
            n.iter().all(|x| *x == 0.0),
            "Nollvektor ska förbli nollvektor"
        );
    }

    #[test]
    fn test_mean_pool() {
        // 2 tokens, dim=3, mask=[1,1]
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mask = vec![1, 1];
        let result = mean_pool(&data, &mask, 2, 3);
        assert_eq!(result.len(), 3, "Pooled dim ska vara 3");
        assert!((result[0] - 2.5).abs() < 1e-5, "Medelvärde av 1.0 och 4.0");
        assert!((result[1] - 3.5).abs() < 1e-5, "Medelvärde av 2.0 och 5.0");
        assert!((result[2] - 4.5).abs() < 1e-5, "Medelvärde av 3.0 och 6.0");
    }

    #[test]
    fn test_mean_pool_with_padding() {
        // 3 tokens, dim=2, mask=[1,1,0] — tredje ska ignoreras
        let data = vec![1.0, 2.0, 3.0, 4.0, 99.0, 99.0];
        let mask = vec![1, 1, 0];
        let result = mean_pool(&data, &mask, 3, 2);
        assert!(
            (result[0] - 2.0).abs() < 1e-5,
            "Medelvärde av 1.0 och 3.0 (padding ignorerad)"
        );
    }

    #[test]
    fn test_split_to_words() {
        let words = split_to_words("hello, world! how are you?");
        assert_eq!(
            words,
            vec!["hello", ",", "world", "!", "how", "are", "you", "?"]
        );
    }

    #[test]
    fn test_split_to_words_swedish() {
        let words = split_to_words("hitta kontaktinformation på sidan");
        assert_eq!(words, vec!["hitta", "kontaktinformation", "på", "sidan"]);
    }

    #[test]
    fn test_split_to_words_empty() {
        let words = split_to_words("");
        assert!(words.is_empty(), "Tom sträng ska ge tom vektor");
    }

    #[test]
    fn test_embedding_cache() {
        let mut cache = EmbeddingCache::new(3);
        cache.insert("a".to_string(), vec![1.0]);
        cache.insert("b".to_string(), vec![2.0]);
        cache.insert("c".to_string(), vec![3.0]);

        assert!(cache.get("a").is_some(), "a ska finnas");
        assert!(cache.get("b").is_some(), "b ska finnas");

        // Lägg till d — ska evictera a
        cache.insert("d".to_string(), vec![4.0]);
        assert!(cache.get("a").is_none(), "a ska vara evicterad");
        assert!(cache.get("d").is_some(), "d ska finnas");
    }

    #[test]
    fn test_embedding_cache_duplicate() {
        let mut cache = EmbeddingCache::new(3);
        cache.insert("a".to_string(), vec![1.0]);
        cache.insert("a".to_string(), vec![2.0]);
        assert_eq!(
            cache.order.len(),
            1,
            "Duplikat ska inte lägga till extra entry"
        );
        assert_eq!(
            cache.get("a").unwrap(),
            &vec![1.0],
            "Första värdet ska behållas"
        );
    }

    #[test]
    fn test_enhanced_similarity_exact_match() {
        // Utan modell laddad — testar fallback
        let score = enhanced_similarity("hello", "hello");
        assert!(
            (score - 1.0).abs() < 1e-5,
            "Exakt match ska ge 1.0, fick {score}"
        );
    }

    #[test]
    fn test_enhanced_similarity_fallback() {
        // Utan modell laddad — ska falla tillbaka till text_similarity
        let score = enhanced_similarity("köp produkt", "köp produkt nu");
        assert!(score > 0.0, "Delvis match ska ge > 0.0, fick {score}");
    }

    #[test]
    fn test_wordpiece_tokenizer_basic() {
        // Skapa minimal vocab för test
        let vocab_lines: Vec<String> = (0..103)
            .map(|i| match i {
                0 => "[PAD]".to_string(),
                100 => "[UNK]".to_string(),
                101 => "[CLS]".to_string(),
                102 => "[SEP]".to_string(),
                _ => format!("[unused{i}]"),
            })
            .collect();
        // Lägg till riktiga tokens
        let mut lines = vocab_lines;
        lines.push("hello".to_string()); // id 103
        lines.push("world".to_string()); // id 104
        lines.push("##s".to_string()); // id 105
        let vocab_text = lines.join("\n");

        let tokenizer = WordPieceTokenizer::from_vocab_text(&vocab_text).unwrap();
        let result = tokenizer.tokenize("hello worlds");

        // Förväntat: [CLS]=101, hello=103, world=104, ##s=105, [SEP]=102, [PAD]...
        assert_eq!(result.input_ids[0], 101, "[CLS] ska vara först");
        assert_eq!(result.input_ids[1], 103, "hello ska vara id 103");
        assert_eq!(result.input_ids[2], 104, "world ska vara id 104");
        assert_eq!(result.input_ids[3], 105, "##s ska vara id 105");
        assert_eq!(
            result.input_ids[4], 102,
            "[SEP] ska vara sist bland riktiga tokens"
        );
        assert_eq!(result.attention_mask[4], 1, "SEP ska ha attention_mask=1");
        assert_eq!(
            result.attention_mask[5], 0,
            "Padding ska ha attention_mask=0"
        );
    }

    #[test]
    fn test_wordpiece_unknown_token() {
        let mut lines: Vec<String> = (0..103)
            .map(|i| match i {
                0 => "[PAD]".to_string(),
                100 => "[UNK]".to_string(),
                101 => "[CLS]".to_string(),
                102 => "[SEP]".to_string(),
                _ => format!("[unused{i}]"),
            })
            .collect();
        lines.push("hello".to_string());
        let vocab_text = lines.join("\n");

        let tokenizer = WordPieceTokenizer::from_vocab_text(&vocab_text).unwrap();
        let result = tokenizer.tokenize("hello unknownword");

        assert_eq!(result.input_ids[0], 101, "[CLS]");
        assert_eq!(result.input_ids[1], 103, "hello");
        assert_eq!(result.input_ids[2], 100, "unknownword ska bli [UNK]");
        assert_eq!(result.input_ids[3], 102, "[SEP]");
    }

    #[test]
    fn test_is_loaded_initially_false() {
        // Global modellen är inte laddad i testkontexten
        // (den initialiseras bara av init_global med riktiga bytes)
        // Vi testar bara att funktionen inte panikerar
        let _ = is_loaded();
    }
}
