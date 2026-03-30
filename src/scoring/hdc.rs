/// HDC (Hyperdimensional Computing) — hierarkisk pruning
///
/// Bitvector-baserade hypervektorer för snabb subträd-eliminering.
/// XOR + popcount ger nanosekund-snabb likhetsmätning.

/// Dimensionalitet för hypervektorer (antal bits)
pub const HDC_DIM: usize = 1024;
/// Antal u64-ord per hypervector
const WORDS: usize = HDC_DIM / 64;

/// En hypervector representerad som en bitvector av u64-ord
#[derive(Clone)]
pub struct Hypervector {
    bits: [u64; WORDS],
}

/// HDC-träd med en hypervector per nod
pub struct HdcTree {
    nodes: Vec<(u32, Hypervector)>,
}

impl HdcTree {
    pub fn new() -> Self {
        HdcTree { nodes: Vec::new() }
    }
}
