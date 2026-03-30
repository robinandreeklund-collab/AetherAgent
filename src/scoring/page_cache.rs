// Page-level cache — cachar hämtad HTML + SemanticTree per URL
//
// Amorterar fetch (~200ms) + parse (~100ms) vid upprepade queries
// mot samma sida. TTL-baserad invalidering per sidtyp.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::types::SemanticTree;

/// Max antal cachade sidor
const MAX_PAGE_ENTRIES: usize = 64;

/// Default TTL (5 minuter)
const DEFAULT_TTL_SECS: u64 = 300;

/// Cachad sida med TTL
pub struct CachedPage {
    pub html: String,
    pub tree: SemanticTree,
    pub fetched_at: Instant,
    pub ttl: Duration,
}

impl CachedPage {
    pub fn is_expired(&self) -> bool {
        self.fetched_at.elapsed() > self.ttl
    }
}

/// Global page-cache
static PAGE_CACHE: std::sync::OnceLock<Mutex<PageCache>> = std::sync::OnceLock::new();

struct PageCache {
    entries: HashMap<String, CachedPage>,
    access_order: Vec<String>,
}

impl PageCache {
    fn new() -> Self {
        PageCache {
            entries: HashMap::with_capacity(MAX_PAGE_ENTRIES),
            access_order: Vec::with_capacity(MAX_PAGE_ENTRIES),
        }
    }

    fn get(&mut self, url: &str) -> Option<&CachedPage> {
        // Kolla om expired
        if let Some(entry) = self.entries.get(url) {
            if entry.is_expired() {
                self.entries.remove(url);
                self.access_order.retain(|k| k != url);
                return None;
            }
        }

        if self.entries.contains_key(url) {
            self.access_order.retain(|k| k != url);
            self.access_order.push(url.to_string());
            self.entries.get(url)
        } else {
            None
        }
    }

    fn insert(&mut self, url: String, page: CachedPage) {
        while self.entries.len() >= MAX_PAGE_ENTRIES {
            if let Some(oldest) = self.access_order.first().cloned() {
                self.entries.remove(&oldest);
                self.access_order.remove(0);
            } else {
                break;
            }
        }
        self.access_order.push(url.clone());
        self.entries.insert(url, page);
    }
}

fn global_page_cache() -> &'static Mutex<PageCache> {
    PAGE_CACHE.get_or_init(|| Mutex::new(PageCache::new()))
}

/// TTL baserad på URL-mönster
pub fn ttl_for_url(url: &str) -> Duration {
    let lower = url.to_lowercase();

    // Statiska faktasidor
    if lower.contains("wikipedia.org")
        || lower.contains("britannica.com")
        || lower.contains("docs.rs")
    {
        return Duration::from_secs(600); // 10 min
    }

    // Myndighetssidor
    if lower.contains(".gov") || lower.contains("riksdagen.se") {
        return Duration::from_secs(300); // 5 min
    }

    // Nyhets- och realtid
    if lower.contains("news")
        || lower.contains("reuters")
        || lower.contains("bbc.com")
        || lower.contains("cnn.com")
    {
        return Duration::from_secs(30); // 30 sek
    }

    // SPA/JS-renderade
    if lower.contains("worldpopulationreview")
        || lower.contains("premierleague")
        || lower.contains("npmjs.com")
    {
        return Duration::from_secs(60); // 1 min
    }

    Duration::from_secs(DEFAULT_TTL_SECS)
}

/// Hämta cachad sida om tillgänglig (och ej expired)
pub fn get_cached_page(url: &str) -> Option<(String, SemanticTree)> {
    let mut cache = global_page_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cache.get(url).map(|p| (p.html.clone(), p.tree.clone()))
}

/// Cacha en ny sida
pub fn cache_page(url: &str, html: &str, tree: &SemanticTree) {
    let ttl = ttl_for_url(url);
    let page = CachedPage {
        html: html.to_string(),
        tree: tree.clone(),
        fetched_at: Instant::now(),
        ttl,
    };
    let mut cache = global_page_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    cache.insert(url.to_string(), page);
}

/// Rensa page-cachen
pub fn clear_page_cache() {
    if let Ok(mut cache) = global_page_cache().lock() {
        cache.entries.clear();
        cache.access_order.clear();
    }
}

/// Antal cachade sidor
pub fn page_cache_size() -> usize {
    global_page_cache()
        .lock()
        .map(|c| c.entries.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_for_url_wikipedia() {
        let ttl = ttl_for_url("https://en.wikipedia.org/wiki/Stockholm");
        assert_eq!(
            ttl,
            Duration::from_secs(600),
            "Wikipedia borde ha 10 min TTL"
        );
    }

    #[test]
    fn test_ttl_for_url_news() {
        let ttl = ttl_for_url("https://www.bbc.com/news");
        assert_eq!(ttl, Duration::from_secs(30), "BBC News borde ha 30 sek TTL");
    }

    #[test]
    fn test_ttl_for_url_default() {
        let ttl = ttl_for_url("https://example.com");
        assert_eq!(
            ttl,
            Duration::from_secs(300),
            "Default TTL borde vara 5 min"
        );
    }

    #[test]
    fn test_page_cache_clear() {
        clear_page_cache();
        assert_eq!(page_cache_size(), 0);
    }
}
