/// BM25 Index — Okapi BM25 kandidatretrieval (ersätter TF-IDF)
///
/// BM25 ger bättre ranking än ren TF-IDF tack vare:
/// - Term frequency saturation (k1): förhindrar att en term som upprepas
///   100 gånger dominerar — diminishing returns efter ~3-5 förekomster
/// - Document length normalization (b): korta, koncisa noder bootas
///   relativt långa wrapper-noder med utspädd text
/// - Bättre IDF-formel: ln((N - df + 0.5) / (df + 0.5) + 1) ger
///   positivt värde även för vanliga termer (till skillnad från TF-IDF
///   som ger 0 för termer i >50% av noderna)
///
/// Parametrar: k1=1.2, b=0.75 (standard Okapi BM25)
use std::collections::{HashMap, HashSet};

use crate::types::SemanticNode;

/// BM25 term frequency saturation parameter
const BM25_K1: f32 = 1.2;
/// BM25 document length normalization parameter
const BM25_B: f32 = 0.75;

/// BM25-index över semantiska noder (drop-in ersättning för TfIdfIndex)
#[derive(Debug, Clone)]
pub struct TfIdfIndex {
    /// term → lista av (node_id, raw_tf) — BM25-score beräknas vid query-tid
    postings: HashMap<String, Vec<(u32, f32)>>,
    /// document frequency per term
    df: HashMap<String, usize>,
    /// document length per nod (antal termer)
    doc_len: HashMap<u32, f32>,
    /// genomsnittlig dokumentlängd
    avg_dl: f32,
    /// Antal noder i indexet
    node_count: usize,
}

impl TfIdfIndex {
    /// Bygg BM25-index från en platt lista av (node_id, label)
    pub fn build(nodes: &[(u32, &str)]) -> Self {
        let n = nodes.len();
        if n == 0 {
            return TfIdfIndex {
                postings: HashMap::new(),
                df: HashMap::new(),
                doc_len: HashMap::new(),
                avg_dl: 0.0,
                node_count: 0,
            };
        }

        let mut df: HashMap<String, usize> = HashMap::new();
        let mut postings: HashMap<String, Vec<(u32, f32)>> = HashMap::new();
        let mut doc_len: HashMap<u32, f32> = HashMap::new();
        let mut total_terms: f32 = 0.0;

        for &(node_id, label) in nodes {
            let terms = tokenize(label);
            if terms.is_empty() {
                continue;
            }

            let dl = terms.len() as f32;
            doc_len.insert(node_id, dl);
            total_terms += dl;

            // Term frequency per nod
            let mut tf: HashMap<String, f32> = HashMap::new();
            for term in &terms {
                *tf.entry(term.clone()).or_insert(0.0) += 1.0;
            }

            // Document frequency: varje unik term räknas en gång per dokument
            let unique_terms: HashSet<&String> = tf.keys().collect();
            for term in unique_terms {
                *df.entry(term.clone()).or_insert(0) += 1;
            }

            // Lagra raw TF i postings (BM25-score beräknas vid query-tid)
            for (term, freq) in tf {
                postings.entry(term).or_default().push((node_id, freq));
            }
        }

        let docs_with_terms = doc_len.len().max(1) as f32;
        let avg_dl = total_terms / docs_with_terms;

        TfIdfIndex {
            postings,
            df,
            doc_len,
            avg_dl,
            node_count: n,
        }
    }

    /// BM25 query: returnerar node_ids rankade efter BM25-score.
    ///
    /// Steg 1: exakt token-match med BM25-scoring.
    /// Steg 2: prefix-match fallback om steg 1 ger 0 resultat.
    pub fn query(&self, goal: &str, top_k: usize) -> Vec<(u32, f32)> {
        let tokens = tokenize(goal);
        if tokens.is_empty() {
            return vec![];
        }

        let n = self.node_count as f32;
        let mut scores: HashMap<u32, f32> = HashMap::new();

        // Steg 1: Exakt token-match med BM25
        for token in &tokens {
            if let Some(entries) = self.postings.get(token) {
                let doc_freq = *self.df.get(token).unwrap_or(&1) as f32;
                // BM25 IDF: ln((N - df + 0.5) / (df + 0.5) + 1)
                // Alltid positiv, även för vanliga termer
                let idf = ((n - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();

                for &(node_id, raw_tf) in entries {
                    let dl = self.doc_len.get(&node_id).copied().unwrap_or(1.0);
                    // BM25 TF-komponent: (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl/avgdl))
                    let tf_component = (raw_tf * (BM25_K1 + 1.0))
                        / (raw_tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / self.avg_dl.max(1.0)));
                    *scores.entry(node_id).or_insert(0.0) += idf * tf_component;
                }
            }
        }

        // Steg 2: Prefix-match fallback om exakt match ger 0 resultat
        if scores.is_empty() {
            for token in &tokens {
                if token.len() >= 3 {
                    for (index_term, entries) in &self.postings {
                        if index_term.starts_with(token.as_str())
                            || token.starts_with(index_term.as_str())
                        {
                            let doc_freq = *self.df.get(index_term).unwrap_or(&1) as f32;
                            let idf = ((n - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();

                            for &(node_id, raw_tf) in entries {
                                let dl = self.doc_len.get(&node_id).copied().unwrap_or(1.0);
                                let tf_component = (raw_tf * (BM25_K1 + 1.0))
                                    / (raw_tf
                                        + BM25_K1
                                            * (1.0 - BM25_B + BM25_B * dl / self.avg_dl.max(1.0)));
                                // Reducerad score för prefix-match (70%)
                                *scores.entry(node_id).or_insert(0.0) += idf * tf_component * 0.7;
                            }
                        }
                    }
                }
            }
        }

        let mut ranked: Vec<(u32, f32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.truncate(top_k);
        ranked
    }

    /// Inkrementell uppdatering: ta bort gammal label och lägg till ny.
    pub fn update_node(&mut self, node_id: u32, old_label: &str, new_label: &str) {
        // Ta bort gamla entries
        let old_terms = tokenize(old_label);
        for term in &old_terms {
            if let Some(entries) = self.postings.get_mut(term) {
                entries.retain(|(id, _)| *id != node_id);
                if entries.is_empty() {
                    self.postings.remove(term);
                }
            }
        }
        self.doc_len.remove(&node_id);

        // Lägg till nya entries
        let new_terms = tokenize(new_label);
        if new_terms.is_empty() {
            return;
        }

        let dl = new_terms.len() as f32;
        self.doc_len.insert(node_id, dl);

        let mut tf: HashMap<String, f32> = HashMap::new();
        for term in &new_terms {
            *tf.entry(term.clone()).or_insert(0.0) += 1.0;
        }
        for (term, freq) in tf {
            *self.df.entry(term.clone()).or_insert(0) += 1;
            self.postings.entry(term).or_default().push((node_id, freq));
        }
    }

    /// Ta bort en nod helt från indexet
    pub fn remove_node(&mut self, node_id: u32) {
        for entries in self.postings.values_mut() {
            entries.retain(|(id, _)| *id != node_id);
        }
        self.postings.retain(|_, entries| !entries.is_empty());
        self.doc_len.remove(&node_id);
    }

    /// Antal noder i indexet
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Antal unika termer
    pub fn term_count(&self) -> usize {
        self.postings.len()
    }
}

/// Tokenisera text till lowercase termer, filtrera stopwords och korta ord
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != 'å' && c != 'ä' && c != 'ö')
        .filter(|s| s.len() > 2)
        .filter(|s| !is_stopword(s))
        .map(String::from)
        .collect()
}

/// Minimala stopwords (svenska + engelska)
fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "are"
            | "but"
            | "not"
            | "you"
            | "all"
            | "can"
            | "had"
            | "her"
            | "was"
            | "one"
            | "our"
            | "out"
            | "och"
            | "att"
            | "det"
            | "som"
            | "den"
            | "med"
            | "har"
            | "för"
            | "inte"
            | "var"
            | "ett"
            | "till"
            | "från"
    )
}

/// Flattenera ett semantiskt träd till (node_id, label)-par
pub fn flatten_tree(nodes: &[SemanticNode]) -> Vec<(u32, &str)> {
    let mut result = Vec::new();
    flatten_recursive(nodes, &mut result);
    result
}

fn flatten_recursive<'a>(nodes: &'a [SemanticNode], out: &mut Vec<(u32, &'a str)>) {
    for node in nodes {
        if !node.label.is_empty() {
            out.push((node.id, &node.label));
        }
        flatten_recursive(&node.children, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Antal invånare i Malmö");
        assert!(
            tokens.contains(&"antal".to_string()),
            "Borde innehålla 'antal'"
        );
        assert!(
            tokens.contains(&"invånare".to_string()),
            "Borde innehålla 'invånare'"
        );
        assert!(
            tokens.contains(&"malmö".to_string()),
            "Borde innehålla 'malmö'"
        );
        assert!(
            !tokens.contains(&"i".to_string()),
            "Borde filtrera bort 'i'"
        );
    }

    #[test]
    fn test_tokenize_filters_stopwords() {
        let tokens = tokenize("the quick brown fox and the lazy dog");
        assert!(!tokens.contains(&"the".to_string()), "Borde filtrera 'the'");
        assert!(!tokens.contains(&"and".to_string()), "Borde filtrera 'and'");
        assert!(
            tokens.contains(&"quick".to_string()),
            "Borde behålla 'quick'"
        );
    }

    #[test]
    fn test_build_empty() {
        let index = TfIdfIndex::build(&[]);
        assert_eq!(index.node_count(), 0, "Tomt index borde ha 0 noder");
        assert_eq!(index.term_count(), 0, "Tomt index borde ha 0 termer");
    }

    #[test]
    fn test_build_and_query_single_match() {
        let nodes = vec![
            (1, "367 924 invånare i Malmö kommun"),
            (2, "Cookie-inställningar"),
            (3, "Gå till huvudinnehåll"),
        ];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("invånare Malmö", 5);
        assert!(!results.is_empty(), "Borde hitta minst en kandidat");
        assert_eq!(results[0].0, 1, "Nod 1 (invånare Malmö) borde rankas högst");
    }

    #[test]
    fn test_query_ranking_order() {
        let nodes = vec![
            (1, "weather forecast tomorrow"),
            (2, "population statistics inhabitants count"),
            (3, "inhabitants data for the region today"),
            (4, "cookie settings privacy"),
            (5, "regional statistics overview population"),
        ];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("inhabitants statistics", 10);
        assert!(
            results.len() >= 2,
            "Borde hitta minst 2 kandidater, fick {}",
            results.len()
        );
        assert_eq!(
            results[0].0, 2,
            "Nod 2 (inhabitants + statistics) borde rankas högst"
        );
    }

    #[test]
    fn test_query_no_match() {
        let nodes = vec![(1, "Cookie-inställningar"), (2, "Gå till huvudinnehåll")];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("antal invånare", 5);
        assert!(
            results.is_empty(),
            "Inga matchande termer borde ge tomt resultat"
        );
    }

    #[test]
    fn test_top_k_limit() {
        let nodes: Vec<(u32, &str)> = (0..100).map(|i| (i, "test term example data")).collect();
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("test example", 5);
        assert!(results.len() <= 5, "Borde respektera top_k-begränsning");
    }

    #[test]
    fn test_bm25_common_terms_still_score() {
        // BM25 ska ge positiv score även för vanliga termer (olikt TF-IDF som ger 0)
        let nodes = vec![
            (1, "rust programming language features"),
            (2, "rust compiler error messages"),
            (3, "rust memory safety guarantees"),
            (4, "python scripting language"),
        ];
        let index = TfIdfIndex::build(&nodes);

        // "rust" förekommer i 3/4 noder — TF-IDF gav IDF≈0, BM25 ska ge >0
        let results = index.query("rust", 5);
        assert!(
            results.len() >= 3,
            "BM25 borde hitta alla 3 rust-noder, fick {}",
            results.len()
        );
        // Alla rust-noder borde ha positiv score
        for (id, score) in &results {
            if *id != 4 {
                assert!(
                    *score > 0.0,
                    "Nod {id} med 'rust' borde ha positiv BM25-score, fick {score}"
                );
            }
        }
    }

    #[test]
    fn test_bm25_short_docs_boosted() {
        // BM25 length normalization: korta noder borde rankas högre
        let nodes = vec![
            (1, "Rust download"),
            (2, "Rust is a multi-paradigm general-purpose programming language that emphasizes performance type safety and concurrency"),
        ];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("rust download", 5);
        assert!(
            results.len() >= 2,
            "Borde hitta båda, fick {}",
            results.len()
        );
        // Nod 1 (kort, exakt match) borde rankas högre
        assert_eq!(
            results[0].0, 1,
            "Kort nod med exakt match borde rankas högst"
        );
    }

    #[test]
    fn test_bm25_idf_weighting() {
        // Unik term borde ge högre score än vanlig term
        let nodes = vec![
            (1, "population statistics data"),
            (2, "cookie settings"),
            (3, "weather forecast"),
        ];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("population", 5);
        assert!(!results.is_empty(), "Borde hitta nod med 'population'");
        assert_eq!(
            results[0].0, 1,
            "Nod med unik term 'population' borde rankas högst"
        );
    }

    #[test]
    fn test_update_node_adds_new_terms() {
        let nodes = vec![(1, "weather forecast today"), (2, "cookie settings")];
        let mut index = TfIdfIndex::build(&nodes);

        index.update_node(2, "cookie settings", "population data count");

        let results = index.query("population data", 5);
        assert!(
            results.iter().any(|(id, _)| *id == 2),
            "Nod 2 borde hittas efter uppdatering"
        );

        let old_results = index.query("cookie", 5);
        assert!(
            old_results.iter().all(|(id, _)| *id != 2),
            "Nod 2 borde inte matcha 'cookie' efter uppdatering"
        );
    }

    #[test]
    fn test_remove_node() {
        let nodes = vec![
            (1, "population statistics data"),
            (2, "cookie settings privacy"),
            (3, "weather forecast tomorrow"),
        ];
        let mut index = TfIdfIndex::build(&nodes);

        index.remove_node(2);

        let results = index.query("cookie settings", 5);
        assert!(
            results.iter().all(|(id, _)| *id != 2),
            "Borttagen nod borde inte hittas"
        );

        let results = index.query("statistics", 5);
        assert!(
            results.iter().any(|(id, _)| *id == 1),
            "Nod 1 borde fortfarande hittas"
        );
    }

    #[test]
    fn test_flatten_tree() {
        use crate::types::SemanticNode;
        let tree = vec![SemanticNode {
            id: 1,
            role: "text".into(),
            label: "Rot-nod".into(),
            children: vec![SemanticNode {
                id: 2,
                role: "button".into(),
                label: "Klicka här".into(),
                children: vec![],
                ..SemanticNode::default()
            }],
            ..SemanticNode::default()
        }];
        let flat = flatten_tree(&tree);
        assert_eq!(flat.len(), 2, "Borde platta ut 2 noder");
        assert_eq!(flat[0].0, 1, "Första noden borde ha id 1");
        assert_eq!(flat[1].0, 2, "Andra noden borde ha id 2");
    }
}
