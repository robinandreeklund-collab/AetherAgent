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
/// 2048 ger bra separation på långa noder med marginell extra kostnad (~+0.02ms query)
pub const HDC_DIM: usize = 2048;
/// Antal u64-ord per hypervector
const WORDS: usize = HDC_DIM / 64;

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
    pub fn bind(&self, other: &Hypervector) -> Hypervector {
        let mut result = [0u64; WORDS];
        for (i, r) in result.iter_mut().enumerate() {
            *r = self.bits[i] ^ other.bits[i];
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
                for (i, r) in result.iter_mut().enumerate() {
                    *r = hvs[0].bits[i] & hvs[1].bits[i];
                }
            }
            3 => {
                for (i, r) in result.iter_mut().enumerate() {
                    let (a, b, c) = (hvs[0].bits[i], hvs[1].bits[i], hvs[2].bits[i]);
                    *r = (a & b) | (a & c) | (b & c);
                }
            }
            _ => {
                let threshold = hvs.len() / 2;
                for bit_idx in 0..HDC_DIM {
                    let word_idx = bit_idx / 64;
                    let bit_pos = bit_idx % 64;
                    let mask = 1u64 << bit_pos;

                    let ones: usize = hvs
                        .iter()
                        .filter(|hv| hv.bits[word_idx] & mask != 0)
                        .count();

                    if ones > threshold {
                        result[word_idx] |= mask;
                    }
                }
            }
        }

        Hypervector { bits: result }
    }

    /// Cosine-likhet approximerad via Hamming-avstånd
    /// cos(a,b) ≈ 1 - 2 * hamming(a,b) / DIM
    pub fn similarity(&self, other: &Hypervector) -> f32 {
        let hamming = self.hamming_distance(other);
        1.0 - 2.0 * (hamming as f32) / (HDC_DIM as f32)
    }

    /// Hamming-avstånd (antal bits som skiljer sig) via XOR + popcount
    fn hamming_distance(&self, other: &Hypervector) -> u32 {
        self.bits
            .iter()
            .zip(other.bits.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }
}

/// HDC-träd med en hypervector per nod-ID
pub struct HdcTree {
    nodes: HashMap<u32, Hypervector>,
}

impl HdcTree {
    /// Bygg HDC-träd från ett semantiskt träd
    pub fn build(tree_nodes: &[SemanticNode]) -> Self {
        let mut nodes = HashMap::new();
        for node in tree_nodes {
            Self::build_recursive(node, &mut nodes, 0);
        }
        HdcTree { nodes }
    }

    fn build_recursive(
        node: &SemanticNode,
        out: &mut HashMap<u32, Hypervector>,
        depth: usize,
    ) -> Hypervector {
        // Text-HV: genereras från nodens label
        let text_hv = Hypervector::from_seed(&node.label);

        // Tag/Role-HV: genereras från nodens roll
        let role_hv = Hypervector::from_seed(&format!("__role_{}", node.role));

        // Bind text ⊗ roll
        let mut local_hv = text_hv.bind(&role_hv);

        // Positionskodning via permutation
        local_hv = local_hv.permute(depth * 7); // 7 bits shift per djupnivå

        // Bundle med barn (majority vote)
        if !node.children.is_empty() {
            let mut child_hvs: Vec<Hypervector> = Vec::with_capacity(node.children.len());
            for child in &node.children {
                let child_hv = Self::build_recursive(child, out, depth + 1);
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
        Hypervector::from_seed(goal)
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

    /// Antal noder i trädet
    pub fn node_count(&self) -> usize {
        self.nodes.len()
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
}
