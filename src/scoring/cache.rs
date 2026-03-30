// Cache för TF-IDF och HDC build-faser
//
// Amorterar ~6ms build-kostnad vid upprepade queries mot samma sida.
// Nyckel: FNV-hash av HTML-innehåll. Bounded LRU (max 32 entries).
// Arc-wrapped data — clone är billigt (referensräknare).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::scoring::embed_score::{build_node_index, NodeInfo};
use crate::scoring::hdc::HdcTree;
use crate::scoring::tfidf::{self, TfIdfIndex};
use crate::types::SemanticNode;

/// Max antal cachade sidor
const MAX_CACHE_ENTRIES: usize = 32;

/// Cachad build-data för en sida (Arc-wrapped, billigt att klona)
#[derive(Clone)]
pub struct CachedBuild {
    pub tfidf_index: Arc<TfIdfIndex>,
    pub hdc_tree: Arc<HdcTree>,
    pub node_index: Arc<HashMap<u32, NodeInfo>>,
    pub build_tfidf_us: u64,
    pub build_hdc_us: u64,
}

/// Global scoring-cache (thread-safe via Mutex)
static SCORING_CACHE: std::sync::OnceLock<Mutex<ScoringCache>> = std::sync::OnceLock::new();

struct ScoringCache {
    entries: HashMap<u64, CachedBuild>,
    /// Access-order för LRU-eviction (senast använda sist)
    access_order: Vec<u64>,
}

impl ScoringCache {
    fn new() -> Self {
        ScoringCache {
            entries: HashMap::with_capacity(MAX_CACHE_ENTRIES),
            access_order: Vec::with_capacity(MAX_CACHE_ENTRIES),
        }
    }

    fn get(&mut self, key: u64) -> Option<CachedBuild> {
        if let Some(entry) = self.entries.get(&key) {
            // Flytta till slutet (senast använd)
            self.access_order.retain(|&k| k != key);
            self.access_order.push(key);
            Some(entry.clone()) // Arc-clone: billigt
        } else {
            None
        }
    }

    fn insert(&mut self, key: u64, value: CachedBuild) {
        // Evicta äldsta om fullt
        while self.entries.len() >= MAX_CACHE_ENTRIES {
            if let Some(oldest_key) = self.access_order.first().copied() {
                self.entries.remove(&oldest_key);
                self.access_order.remove(0);
            } else {
                break;
            }
        }
        self.access_order.push(key);
        self.entries.insert(key, value);
    }
}

fn global_cache() -> &'static Mutex<ScoringCache> {
    SCORING_CACHE.get_or_init(|| Mutex::new(ScoringCache::new()))
}

/// FNV-1a hash av HTML-innehåll
fn content_hash(html: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in html.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Resultat från cache-lookup eller build
pub struct BuildResult {
    pub tfidf_index: Arc<TfIdfIndex>,
    pub hdc_tree: Arc<HdcTree>,
    pub node_index: Arc<HashMap<u32, NodeInfo>>,
    pub build_tfidf_us: u64,
    pub build_hdc_us: u64,
    pub cache_hit: bool,
}

/// Cache-aware build: returnerar cachad build om tillgänglig, annars bygger och cachar.
pub fn get_or_build(html: &str, tree_nodes: &[SemanticNode]) -> BuildResult {
    let key = content_hash(html);

    // Försök hämta från cache
    {
        let mut cache = global_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(key) {
            return BuildResult {
                tfidf_index: cached.tfidf_index,
                hdc_tree: cached.hdc_tree,
                node_index: cached.node_index,
                build_tfidf_us: cached.build_tfidf_us,
                build_hdc_us: cached.build_hdc_us,
                cache_hit: true,
            };
        }
    }

    // Cache-miss: sekventiell build (se pipeline.rs för rayon-motivering)
    let t0 = Instant::now();
    let flat_nodes = tfidf::flatten_tree(tree_nodes);
    let tfidf_index = Arc::new(TfIdfIndex::build(&flat_nodes));
    let build_tfidf_us = t0.elapsed().as_micros() as u64;

    let t1 = Instant::now();
    let hdc_tree = Arc::new(HdcTree::build(tree_nodes));
    let build_hdc_us = t1.elapsed().as_micros() as u64;

    let node_index = Arc::new(build_node_index(tree_nodes));

    // Cacha för framtida anrop
    let cached = CachedBuild {
        tfidf_index: Arc::clone(&tfidf_index),
        hdc_tree: Arc::clone(&hdc_tree),
        node_index: Arc::clone(&node_index),
        build_tfidf_us,
        build_hdc_us,
    };

    {
        let mut cache = global_cache().lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, cached);
    }

    BuildResult {
        tfidf_index,
        hdc_tree,
        node_index,
        build_tfidf_us,
        build_hdc_us,
        cache_hit: false,
    }
}

/// Rensa scoring-cachen (för test / minnesåterhämtning)
pub fn clear_cache() {
    if let Ok(mut cache) = global_cache().lock() {
        cache.entries.clear();
        cache.access_order.clear();
    }
}

/// Antal cachade entries
pub fn cache_size() -> usize {
    global_cache().lock().map(|c| c.entries.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2, "Samma input borde ge samma hash");
    }

    #[test]
    fn test_content_hash_different() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2, "Olika input borde ge olika hash");
    }

    #[test]
    fn test_cache_clear() {
        clear_cache();
        assert_eq!(cache_size(), 0, "Cache borde vara tom efter clear");
    }

    #[test]
    fn test_get_or_build_caches() {
        clear_cache();

        let html = "<html><body><p>Test content</p></body></html>";
        let nodes = vec![SemanticNode {
            id: 1,
            role: "text".into(),
            label: "Test content".into(),
            children: vec![],
            ..SemanticNode::default()
        }];

        // Första anrop: cache-miss
        let result1 = get_or_build(html, &nodes);
        assert!(!result1.cache_hit, "Första anropet borde vara cache-miss");
        assert_eq!(cache_size(), 1, "Cache borde ha 1 entry");

        // Andra anrop: cache-hit
        let result2 = get_or_build(html, &nodes);
        assert!(result2.cache_hit, "Andra anropet borde vara cache-hit");
        assert_eq!(cache_size(), 1, "Cache borde fortfarande ha 1 entry");

        // Annan HTML: cache-miss
        let html2 = "<html><body><p>Different content</p></body></html>";
        let result3 = get_or_build(html2, &nodes);
        assert!(!result3.cache_hit, "Ny HTML borde vara cache-miss");
        assert_eq!(cache_size(), 2, "Cache borde ha 2 entries");
    }
}
