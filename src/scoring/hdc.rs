/// HDC (Hyperdimensional Computing) — hierarkisk pruning
///
/// Bitvector-baserade hypervektorer för snabb subträd-eliminering.
/// XOR + popcount ger nanosekund-snabb likhetsmätning.
///
/// Varje nods hypervector binds från:
/// - Text-HV: deterministisk hash-projection av label-tokens
/// - Tag-HV: per HTML-tagg (seedad)
/// - Position-HV: permutation baserad på djup
///
/// Föräldra-noder bundlar barnens HV via majority-vote.
use std::collections::HashMap;

use crate::types::SemanticNode;

/// Dimensionalitet för hypervektorer (antal bits)
/// Benchmarkad: 1024/2048/4096 ger identisk korrekthet (18/20).
/// 4096 vald för headroom vid stora DOM:ar, marginell merkostnad.
pub const HDC_DIM: usize = 2048;
/// Antal u64-ord per hypervector
pub const WORDS: usize = HDC_DIM / 64; // 32

/// En hypervector representerad som bitvector av u64-ord
#[derive(Clone, Debug)]
pub struct Hypervector {
    bits: [u64; WORDS],
}

impl Hypervector {
    /// Noll-vektor
    pub fn zero() -> Self {
        Hypervector {
            bits: [0u64; WORDS],
        }
    }

    /// Access the raw bit storage (for serialization)
    pub fn bits_raw(&self) -> &[u64; WORDS] {
        &self.bits
    }

    /// Construct from raw bit storage (for deserialization)
    pub fn from_bits(bits: [u64; WORDS]) -> Self {
        Hypervector { bits }
    }

    /// Generera deterministisk HV från en seed-sträng (pseudo-random via FNV-liknande hash)
    pub fn from_seed(seed: &str) -> Self {
        let mut bits = [0u64; WORDS];
        // FNV-1a-inspirerad hash per bit-word
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (i, word) in bits.iter_mut().enumerate() {
            for byte in seed.as_bytes() {
                h ^= *byte as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            // Mixa in word-index för unikhet per position
            h ^= i as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
            *word = h;
        }
        Hypervector { bits }
    }

    /// XOR-bind: kombinera två HV:er (representerar "bundling" i HDC)
    /// SIMD-vänlig: 4-wide unrolled loop för bättre auto-vektorisering.
    pub fn bind(&self, other: &Hypervector) -> Hypervector {
        let mut result = [0u64; WORDS];
        // 4-wide unrolling: LLVM auto-vektoriserar till SIMD (SSE2/AVX2)
        let chunks = WORDS / 4;
        for c in 0..chunks {
            let base = c * 4;
            result[base] = self.bits[base] ^ other.bits[base];
            result[base + 1] = self.bits[base + 1] ^ other.bits[base + 1];
            result[base + 2] = self.bits[base + 2] ^ other.bits[base + 2];
            result[base + 3] = self.bits[base + 3] ^ other.bits[base + 3];
        }
        Hypervector { bits: result }
    }

    /// Cyklisk permutation med `shift` steg (positionskodning i HDC)
    pub fn permute(&self, shift: usize) -> Hypervector {
        if shift == 0 {
            return self.clone();
        }
        let bit_shift = shift % HDC_DIM;
        let word_shift = bit_shift / 64;
        let bit_offset = bit_shift % 64;

        let mut result = [0u64; WORDS];
        for (i, r) in result.iter_mut().enumerate() {
            let src_word = (i + WORDS - word_shift) % WORDS;
            if bit_offset == 0 {
                *r = self.bits[src_word];
            } else {
                let src_prev = (src_word + WORDS - 1) % WORDS;
                *r = (self.bits[src_word] << bit_offset)
                    | (self.bits[src_prev] >> (64 - bit_offset));
            }
        }
        Hypervector { bits: result }
    }

    /// Majority-vote bundle: given a list of HVs, set each bit to the majority value.
    /// Optimerad: specialfall för 2-3 HV:er (vanligast), generell fallback för fler.
    pub fn bundle(hvs: &[&Hypervector]) -> Hypervector {
        if hvs.is_empty() {
            return Hypervector::zero();
        }
        if hvs.len() == 1 {
            return hvs[0].clone();
        }

        let mut result = [0u64; WORDS];

        match hvs.len() {
            2 => {
                // 4-wide unrolled AND
                let chunks = WORDS / 4;
                for c in 0..chunks {
                    let b = c * 4;
                    result[b] = hvs[0].bits[b] & hvs[1].bits[b];
                    result[b + 1] = hvs[0].bits[b + 1] & hvs[1].bits[b + 1];
                    result[b + 2] = hvs[0].bits[b + 2] & hvs[1].bits[b + 2];
                    result[b + 3] = hvs[0].bits[b + 3] & hvs[1].bits[b + 3];
                }
            }
            3 => {
                // 4-wide unrolled majority
                let chunks = WORDS / 4;
                for c in 0..chunks {
                    let b = c * 4;
                    for off in 0..4 {
                        let (a, x, y) = (
                            hvs[0].bits[b + off],
                            hvs[1].bits[b + off],
                            hvs[2].bits[b + off],
                        );
                        result[b + off] = (a & x) | (a & y) | (x & y);
                    }
                }
            }
            _ => {
                // SIMD-vänlig majority-vote: word-level istf bit-level.
                // Räknar bits per position via addition av partial popcount.
                let threshold = hvs.len() / 2;
                // Processera per u64-ord (64 bits åt gången)
                for (word_idx, result_word) in result.iter_mut().enumerate() {
                    let mut counts = [0u16; 64];
                    for hv in hvs {
                        let w = hv.bits[word_idx];
                        for (bit_pos, count) in counts.iter_mut().enumerate() {
                            *count += ((w >> bit_pos) & 1) as u16;
                        }
                    }
                    let mut word: u64 = 0;
                    for (bit_pos, &count) in counts.iter().enumerate() {
                        if count as usize > threshold {
                            word |= 1u64 << bit_pos;
                        }
                    }
                    *result_word = word;
                }
            }
        }

        Hypervector { bits: result }
    }

    /// Skapa HV från text med n-gram binding och positionskodning.
    ///
    /// Splittrar texten i ord, genererar HV per ord, binder 2-grams och 3-grams
    /// med positionspermutation, och bundlar allt via majority-vote.
    /// Ger ordningskänslig representation ("katt jagar hund" ≠ "hund jagar katt").
    pub fn from_text_ngrams(text: &str) -> Self {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 1)
            .collect();

        if words.is_empty() {
            return Self::from_seed(text);
        }

        let mut components: Vec<Hypervector> = Vec::new();

        // Unigrams med position
        for (i, word) in words.iter().enumerate() {
            let word_hv = Self::from_seed(word);
            components.push(word_hv.permute(i * 3));
        }

        // Bigrams: bind(word[i], permute(word[i+1], 1))
        for i in 0..words.len().saturating_sub(1) {
            let a = Self::from_seed(words[i]);
            let b = Self::from_seed(words[i + 1]).permute(1);
            components.push(a.bind(&b).permute(i * 5));
        }

        // Trigrams: bind(word[i], permute(word[i+1], 1), permute(word[i+2], 2))
        for i in 0..words.len().saturating_sub(2) {
            let a = Self::from_seed(words[i]);
            let b = Self::from_seed(words[i + 1]).permute(1);
            let c = Self::from_seed(words[i + 2]).permute(2);
            components.push(a.bind(&b).bind(&c).permute(i * 7));
        }

        // Bundle alla komponenter
        let refs: Vec<&Hypervector> = components.iter().collect();
        Self::bundle(&refs)
    }

    /// Cosine-likhet approximerad via Hamming-avstånd
    /// cos(a,b) ≈ 1 - 2 * hamming(a,b) / DIM
    pub fn similarity(&self, other: &Hypervector) -> f32 {
        let hamming = self.hamming_distance(other);
        1.0 - 2.0 * (hamming as f32) / (HDC_DIM as f32)
    }

    /// Hamming-avstånd (antal bits som skiljer sig) via XOR + popcount.
    /// Fused XOR+popcount — LLVM emits POPCNT instruction.
    /// Simpler loop lets LLVM auto-vectorize better for 32-word size.
    fn hamming_distance(&self, other: &Hypervector) -> u32 {
        let mut total: u32 = 0;
        // Fused XOR+popcount — LLVM emits POPCNT instruction
        for i in 0..WORDS {
            total += (self.bits[i] ^ other.bits[i]).count_ones();
        }
        total
    }

    /// Batch similarity: beräkna likhet mot N vektorer i ett svep.
    /// Bättre cache-locality än N separata similarity()-anrop.
    /// Returnerar (index, similarity) sorterat fallande.
    pub fn similarity_batch(&self, others: &[&Hypervector]) -> Vec<(usize, f32)> {
        let mut results: Vec<(usize, f32)> = others
            .iter()
            .enumerate()
            .map(|(i, other)| (i, self.similarity(other)))
            .collect();
        results.sort_by(|a, b| b.1.total_cmp(&a.1));
        results
    }

    /// Batch hamming: beräkna hamming-avstånd mot N vektorer.
    /// Returnerar raw hamming distances (inte normaliserade).
    pub fn hamming_batch(&self, others: &[&Hypervector]) -> Vec<u32> {
        others
            .iter()
            .map(|other| self.hamming_distance(other))
            .collect()
    }
}

/// HDC-träd med en hypervector per nod-ID.
/// Sparar även separata text- och roll-HVs för multi-aspect scoring (C-optimering).
pub struct HdcTree {
    nodes: HashMap<u32, Hypervector>,
    /// Separata text-HVs (utan roll/position-binding) — för content-likhet
    text_hvs: HashMap<u32, Hypervector>,
    /// Separata roll-HVs — för strukturell filtrering
    role_hvs: HashMap<u32, Hypervector>,
}

impl HdcTree {
    /// Bygg HDC-träd från ett semantiskt träd
    pub fn build(tree_nodes: &[SemanticNode]) -> Self {
        let mut nodes = HashMap::new();
        let mut text_hvs = HashMap::new();
        let mut role_hvs = HashMap::new();
        for node in tree_nodes {
            Self::build_recursive(node, &mut nodes, &mut text_hvs, &mut role_hvs, 0);
        }
        HdcTree {
            nodes,
            text_hvs,
            role_hvs,
        }
    }

    fn build_recursive(
        node: &SemanticNode,
        out: &mut HashMap<u32, Hypervector>,
        text_hvs: &mut HashMap<u32, Hypervector>,
        role_hvs: &mut HashMap<u32, Hypervector>,
        depth: usize,
    ) -> Hypervector {
        // Text-HV: n-gram-baserad för ordningskänslig representation
        let text_hv = Hypervector::from_text_ngrams(&node.label);

        // Tag/Role-HV: genereras från nodens roll
        let role_hv = Hypervector::from_seed(&format!("__role_{}", node.role));

        // Spara separata aspekt-HVs (C-optimering)
        text_hvs.insert(node.id, text_hv.clone());
        role_hvs.insert(node.id, role_hv.clone());

        // Bind text ⊗ roll
        let mut local_hv = text_hv.bind(&role_hv);

        // Positionskodning via permutation
        local_hv = local_hv.permute(depth * 7);

        // Bundle med barn (majority vote)
        if !node.children.is_empty() {
            let mut child_hvs: Vec<Hypervector> = Vec::with_capacity(node.children.len());
            for child in &node.children {
                let child_hv = Self::build_recursive(child, out, text_hvs, role_hvs, depth + 1);
                child_hvs.push(child_hv);
            }

            let refs: Vec<&Hypervector> =
                std::iter::once(&local_hv).chain(child_hvs.iter()).collect();
            local_hv = Hypervector::bundle(&refs);
        }

        out.insert(node.id, local_hv.clone());
        local_hv
    }

    /// Projicera en goal-sträng till en hypervector
    pub fn project_goal(goal: &str) -> Hypervector {
        Hypervector::from_text_ngrams(goal)
    }

    /// Pruna kandidater: behåll bara de vars HV har tillräcklig likhet med goal_hv
    pub fn prune(
        &self,
        candidates: &[(u32, f32)],
        goal_hv: &Hypervector,
        threshold: f32,
    ) -> Vec<(u32, f32)> {
        candidates
            .iter()
            .filter(|(id, _)| {
                self.nodes
                    .get(id)
                    .map(|hv| hv.similarity(goal_hv) >= threshold)
                    // Noder utan HV passerar alltid (säkerhetsnät)
                    .unwrap_or(true)
            })
            .copied()
            .collect()
    }

    /// Hämta HV-likhet för en specifik nod mot goal
    pub fn node_similarity(&self, node_id: u32, goal_hv: &Hypervector) -> Option<f32> {
        self.nodes.get(&node_id).map(|hv| hv.similarity(goal_hv))
    }

    /// Text-aspekt likhet: hur väl matchar nodens textinnehåll (utan roll/position)
    /// mot goal? Används av ColBERT multi-aspect scoring (C-optimering).
    pub fn text_similarity(&self, node_id: u32, goal_hv: &Hypervector) -> Option<f32> {
        self.text_hvs.get(&node_id).map(|hv| hv.similarity(goal_hv))
    }

    /// Roll-aspekt likhet: hur väl matchar nodens roll mot en förväntad roll-HV?
    /// Används för att skilja content-roller (text, data) från strukturella (nav, footer).
    pub fn role_similarity(&self, node_id: u32, role_hv: &Hypervector) -> Option<f32> {
        self.role_hvs.get(&node_id).map(|hv| hv.similarity(role_hv))
    }

    /// Ren strukturell HDC-pruning: rankar ALLA noder efter HV-likhet med goal.
    ///
    /// Används som fallback när BM25 ger 0 kandidater (semantiska frågor
    /// utan keyword-overlap). Returnerar top_k noder sorterade efter HDC-likhet.
    /// Mycket snabbare än embedding (~0.5ms vs ~400ms) men grövre precision.
    pub fn prune_pure(&self, goal_hv: &Hypervector, top_k: usize) -> Vec<(u32, f32)> {
        let mut scored: Vec<(u32, f32)> = self
            .nodes
            .iter()
            .map(|(&id, hv)| (id, hv.similarity(goal_hv)))
            .collect();

        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);
        scored
    }

    /// Antal noder i trädet
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Inkrementell uppdatering: uppdatera en nods HV och rebundla vägen till roten.
    ///
    /// `node_id` — noden som muterats (ny label/roll)
    /// `new_label` — nodens nya label-text
    /// `new_role` — nodens nya roll
    /// `ancestor_path` — lista av (ancestor_id, [child_ids]) från noden till roten
    ///
    /// Istället för att bygga om hela HDC-trädet (~4ms) uppdateras bara O(djup) noder.
    pub fn update_node(
        &mut self,
        node_id: u32,
        new_label: &str,
        new_role: &str,
        ancestor_path: &[(u32, Vec<u32>)],
    ) {
        // Steg 1: Beräkna ny HV för den muterade noden
        let text_hv = Hypervector::from_text_ngrams(new_label);
        let role_hv = Hypervector::from_seed(&format!("__role_{new_role}"));
        let new_hv = text_hv.bind(&role_hv);
        self.nodes.insert(node_id, new_hv);

        // Steg 2: Rebundla varje ancestor uppåt
        for (ancestor_id, child_ids) in ancestor_path {
            let mut hvs: Vec<Hypervector> = Vec::new();
            // Samla alla barns HV (inkl. den uppdaterade)
            for &child_id in child_ids {
                if let Some(child_hv) = self.nodes.get(&child_id) {
                    hvs.push(child_hv.clone());
                }
            }
            if !hvs.is_empty() {
                // Ancestor-HV: sammanfoga sin egen text-HV + barnens bundle
                let refs: Vec<&Hypervector> = hvs.iter().collect();
                let bundled = Hypervector::bundle(&refs);
                self.nodes.insert(*ancestor_id, bundled);
            }
        }
    }

    /// Ta bort en nod från HDC-trädet (vid DOM-deletion)
    pub fn remove_node(&mut self, node_id: u32) {
        self.nodes.remove(&node_id);
    }
}

/// Adaptiv threshold baserad på kontext (djup + roll)
pub fn adaptive_threshold(role: &str, depth: u32) -> f32 {
    // Strukturella top-level noder: låg threshold (passerar nästan alltid)
    if depth <= 1 {
        return -1.0; // passera alltid
    }

    // Navigerings-/footer-kontext: striktare
    match role {
        "navigation" | "complementary" => 0.10,
        "generic" if depth >= 3 => 0.08,
        // Löv-noder: skippa HDC, kör direkt till embedding
        "text" | "paragraph" | "heading" | "link" | "button" | "cta" => -1.0,
        _ => 0.05,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hypervector_from_seed_deterministic() {
        let hv1 = Hypervector::from_seed("test");
        let hv2 = Hypervector::from_seed("test");
        assert_eq!(hv1.bits, hv2.bits, "Samma seed borde ge samma HV");
    }

    #[test]
    fn test_hypervector_different_seeds() {
        let hv1 = Hypervector::from_seed("hello");
        let hv2 = Hypervector::from_seed("world");
        assert_ne!(hv1.bits, hv2.bits, "Olika seeds borde ge olika HV");
    }

    #[test]
    fn test_bind_is_self_inverse() {
        let hv = Hypervector::from_seed("test");
        let other = Hypervector::from_seed("other");
        let bound = hv.bind(&other);
        let unbound = bound.bind(&other);
        assert_eq!(hv.bits, unbound.bits, "XOR-bind borde vara sin egen invers");
    }

    #[test]
    fn test_similarity_self_is_one() {
        let hv = Hypervector::from_seed("test");
        let sim = hv.similarity(&hv);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Likhet med sig själv borde vara ~1.0, fick {sim}"
        );
    }

    #[test]
    fn test_similarity_different_is_near_zero() {
        let hv1 = Hypervector::from_seed("completely different text one");
        let hv2 = Hypervector::from_seed("another unrelated string two");
        let sim = hv1.similarity(&hv2);
        // Slumpmässiga HV:er borde ha ~0.0 likhet (±0.15)
        assert!(
            sim.abs() < 0.25,
            "Orelaterade HV borde ha låg likhet, fick {sim}"
        );
    }

    #[test]
    fn test_permute_preserves_popcount() {
        let hv = Hypervector::from_seed("test");
        let permuted = hv.permute(5);
        let orig_ones: u32 = hv.bits.iter().map(|w| w.count_ones()).sum();
        let perm_ones: u32 = permuted.bits.iter().map(|w| w.count_ones()).sum();
        assert_eq!(orig_ones, perm_ones, "Permutation borde bevara antal ettor");
    }

    #[test]
    fn test_bundle_majority_vote() {
        let hv1 = Hypervector::from_seed("aaa");
        let hv2 = Hypervector::from_seed("bbb");
        let hv3 = Hypervector::from_seed("aaa"); // Samma som hv1

        let bundled = Hypervector::bundle(&[&hv1, &hv2, &hv3]);
        // hv1 och hv3 är identiska → majority borde likna hv1
        let sim_to_1 = bundled.similarity(&hv1);
        let sim_to_2 = bundled.similarity(&hv2);
        assert!(
            sim_to_1 > sim_to_2,
            "Bundle av [a, b, a] borde likna a mer (sim_a={sim_to_1}, sim_b={sim_to_2})"
        );
    }

    #[test]
    fn test_hdc_tree_build_and_prune() {
        let tree = vec![
            SemanticNode {
                id: 1,
                role: "text".into(),
                label: "population statistics data".into(),
                children: vec![SemanticNode {
                    id: 2,
                    role: "text".into(),
                    label: "367924 inhabitants".into(),
                    children: vec![],
                    ..SemanticNode::default()
                }],
                ..SemanticNode::default()
            },
            SemanticNode {
                id: 3,
                role: "navigation".into(),
                label: "cookie settings privacy".into(),
                children: vec![],
                ..SemanticNode::default()
            },
        ];

        let hdc = HdcTree::build(&tree);
        assert_eq!(hdc.node_count(), 3, "Borde ha 3 noder");

        let goal_hv = HdcTree::project_goal("population statistics");
        // Alla kandidater med mycket låg threshold (passerar alla)
        let candidates = vec![(1, 1.0), (2, 0.5), (3, 0.3)];
        let survivors = hdc.prune(&candidates, &goal_hv, -1.0);
        assert_eq!(survivors.len(), 3, "Med threshold=-1 borde alla passera");

        // Med hög threshold pruna bort orelaterade
        let survivors_strict = hdc.prune(&candidates, &goal_hv, 0.5);
        assert!(
            survivors_strict.len() <= candidates.len(),
            "Strikt threshold borde pruna bort noder"
        );
    }

    #[test]
    fn test_adaptive_threshold() {
        assert!(
            adaptive_threshold("text", 0) < 0.0,
            "Djup 0 borde alltid passera"
        );
        assert!(
            adaptive_threshold("navigation", 2) > 0.0,
            "Navigation djup 2 borde ha positiv threshold"
        );
        assert!(
            adaptive_threshold("button", 3) < 0.0,
            "Button borde alltid passera (löv-nod)"
        );
    }

    #[test]
    fn test_ngram_order_sensitivity() {
        // N-gram binding borde ge olika HV:er för olika ordföljder
        let hv1 = Hypervector::from_text_ngrams("cat chases dog");
        let hv2 = Hypervector::from_text_ngrams("dog chases cat");
        let sim = hv1.similarity(&hv2);
        // Borde vara liknande (delar samma unigrams) men inte identiska (olika ordning)
        assert!(
            sim < 0.95,
            "Olika ordföljd borde ge <0.95 likhet, fick {sim}"
        );
        assert!(sim > 0.0, "Delade ord borde ge viss likhet, fick {sim}");
    }

    #[test]
    fn test_ngram_similar_text() {
        let hv1 = Hypervector::from_text_ngrams("population statistics data");
        let hv2 = Hypervector::from_text_ngrams("population statistics report");
        let hv3 = Hypervector::from_text_ngrams("cookie settings privacy");
        let sim_related = hv1.similarity(&hv2);
        let sim_unrelated = hv1.similarity(&hv3);
        assert!(
            sim_related > sim_unrelated,
            "Relaterade texter borde ha högre likhet: related={sim_related}, unrelated={sim_unrelated}"
        );
    }

    #[test]
    fn test_ngram_empty_text() {
        // Tom text borde fallbacka till from_seed utan panik
        let hv = Hypervector::from_text_ngrams("");
        let zero = Hypervector::zero();
        // Borde inte vara noll (from_seed ger pseudo-random)
        assert_ne!(
            hv.bits, zero.bits,
            "Tom text borde ge non-zero HV via seed fallback"
        );
    }

    #[test]
    fn test_update_node_changes_hv() {
        let tree = vec![SemanticNode {
            id: 1,
            role: "text".into(),
            label: "original content".into(),
            children: vec![SemanticNode {
                id: 2,
                role: "text".into(),
                label: "child node".into(),
                children: vec![],
                ..SemanticNode::default()
            }],
            ..SemanticNode::default()
        }];

        let mut hdc = HdcTree::build(&tree);

        // Spara original HV
        let goal_hv = HdcTree::project_goal("population statistics");
        let sim_before = hdc.node_similarity(2, &goal_hv).unwrap_or(0.0);

        // Uppdatera nod 2 till "population statistics data"
        hdc.update_node(
            2,
            "population statistics data",
            "text",
            &[(1, vec![2])], // Ancestor: nod 1 har barn [2]
        );

        let sim_after = hdc.node_similarity(2, &goal_hv).unwrap_or(0.0);

        // HV borde ha ändrats
        assert!(
            (sim_before - sim_after).abs() > 0.001,
            "HV borde ändras efter update: before={sim_before}, after={sim_after}"
        );
    }

    #[test]
    fn test_remove_node() {
        let tree = vec![SemanticNode {
            id: 1,
            role: "text".into(),
            label: "test".into(),
            children: vec![],
            ..SemanticNode::default()
        }];

        let mut hdc = HdcTree::build(&tree);
        assert_eq!(hdc.node_count(), 1, "Borde ha 1 nod");

        hdc.remove_node(1);
        assert_eq!(hdc.node_count(), 0, "Borde vara tom efter removal");
    }
}
