/// TF-IDF Index — snabb keyword-baserad kandidatretrieval
///
/// Bygger ett inverterat index (term → nod-IDs med TF-IDF-score) från
/// det semantiska trädets noder. Query returnerar de top-K noderna
/// vars label har högst TF-IDF-overlap med goal-termen.
///
/// Byggtid: O(n × avg_label_len), querytid: O(|goal_tokens| × avg_postings)
use std::collections::{HashMap, HashSet};

use crate::types::SemanticNode;

/// Inverterat TF-IDF-index över semantiska noder
pub struct TfIdfIndex {
    /// term → lista av (node_id, tf_idf_score)
    index: HashMap<String, Vec<(u32, f32)>>,
    /// Antal noder i indexet
    node_count: usize,
}

impl TfIdfIndex {
    /// Bygg index från en platt lista av noder
    pub fn build(nodes: &[(u32, &str)]) -> Self {
        let n = nodes.len();
        if n == 0 {
            return TfIdfIndex {
                index: HashMap::new(),
                node_count: 0,
            };
        }

        // Steg 1: Beräkna term frequency per nod och document frequency per term
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut tf_per_node: Vec<(u32, HashMap<String, f32>)> = Vec::with_capacity(n);

        for &(node_id, label) in nodes {
            let terms = tokenize(label);
            if terms.is_empty() {
                continue;
            }

            let mut tf: HashMap<String, f32> = HashMap::new();
            for term in &terms {
                *tf.entry(term.clone()).or_insert(0.0) += 1.0;
            }

            // DF: varje unik term räknas en gång per dokument
            let unique_terms: HashSet<&String> = tf.keys().collect();
            for term in unique_terms {
                *df.entry(term.clone()).or_insert(0) += 1;
            }

            tf_per_node.push((node_id, tf));
        }

        // Steg 2: Beräkna TF-IDF och bygg inverterat index
        let n_f32 = n as f32;
        let mut index: HashMap<String, Vec<(u32, f32)>> = HashMap::new();

        for (node_id, tf) in &tf_per_node {
            for (term, &freq) in tf {
                let doc_freq = *df.get(term).unwrap_or(&1) as f32;
                let idf = (n_f32 / (1.0 + doc_freq)).ln();
                // TF normaliseras med log(1+tf) för att dämpa repeteringar
                let tf_norm = (1.0 + freq).ln();
                let score = tf_norm * idf;
                if score > 0.0 {
                    index
                        .entry(term.clone())
                        .or_default()
                        .push((*node_id, score));
                }
            }
        }

        TfIdfIndex {
            index,
            node_count: n,
        }
    }

    /// Sök kandidater: returnerar node_ids rankade efter TF-IDF-likhet med goal
    pub fn query(&self, goal: &str, top_k: usize) -> Vec<(u32, f32)> {
        let tokens = tokenize(goal);
        if tokens.is_empty() {
            return vec![];
        }

        let mut scores: HashMap<u32, f32> = HashMap::new();

        for token in &tokens {
            if let Some(entries) = self.index.get(token) {
                for &(node_id, score) in entries {
                    *scores.entry(node_id).or_insert(0.0) += score;
                }
            }
        }

        let mut ranked: Vec<(u32, f32)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.truncate(top_k);
        ranked
    }

    /// Inkrementell uppdatering: ta bort gammal label och lägg till ny för en nod.
    ///
    /// Enklare än full rebuild — tar bort nodens gamla entries och lägger till nya.
    /// IDF omberäknas inte (approximation som är acceptabel för inkrementella ändringar).
    pub fn update_node(&mut self, node_id: u32, old_label: &str, new_label: &str) {
        // Ta bort gamla entries
        let old_terms = tokenize(old_label);
        for term in &old_terms {
            if let Some(entries) = self.index.get_mut(term) {
                entries.retain(|(id, _)| *id != node_id);
                if entries.is_empty() {
                    self.index.remove(term);
                }
            }
        }

        // Lägg till nya entries (approx TF-IDF, utan full IDF-omberäkning)
        let new_terms = tokenize(new_label);
        let n = self.node_count.max(1) as f32;
        let mut tf: HashMap<String, f32> = HashMap::new();
        for term in &new_terms {
            *tf.entry(term.clone()).or_insert(0.0) += 1.0;
        }
        for (term, freq) in &tf {
            let tf_norm = (1.0 + freq).ln();
            // Approx IDF: använd nuvarande antal entries som DF-estimat
            let df = self.index.get(term).map(|e| e.len()).unwrap_or(0) as f32;
            let idf = (n / (1.0 + df)).ln();
            let score = tf_norm * idf;
            if score > 0.0 {
                self.index
                    .entry(term.clone())
                    .or_default()
                    .push((node_id, score));
            }
        }
    }

    /// Ta bort en nod helt från indexet
    pub fn remove_node(&mut self, node_id: u32) {
        for entries in self.index.values_mut() {
            entries.retain(|(id, _)| *id != node_id);
        }
        self.index.retain(|_, entries| !entries.is_empty());
    }

    /// Antal noder i indexet
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Antal unika termer
    pub fn term_count(&self) -> usize {
        self.index.len()
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

/// Minimala stopwords (svenska + engelska) — termer som inte bör påverka ranking
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
        // "i" filtreras bort (< 3 tecken)
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
        assert!(
            tokens.contains(&"brown".to_string()),
            "Borde behålla 'brown'"
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

        // "inhabitants" förekommer i nod 2 och 3 → medel-IDF
        // "statistics" förekommer i nod 2 och 5 → medel-IDF
        // Nod 2 matchar båda → aggregerad score borde vara högst
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
    fn test_idf_weighting() {
        // "malmö" förekommer i alla noder → låg IDF
        // "invånare" förekommer bara i en nod → hög IDF
        let nodes = vec![
            (1, "Malmö stad information"),
            (2, "367 924 invånare i Malmö kommun"),
            (3, "Malmö centrum shopping"),
        ];
        let index = TfIdfIndex::build(&nodes);

        let results = index.query("invånare", 5);
        assert!(!results.is_empty(), "Borde hitta nod med 'invånare'");
        assert_eq!(
            results[0].0, 2,
            "Nod med unik term 'invånare' borde rankas högst"
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

    #[test]
    fn test_update_node_adds_new_terms() {
        let nodes = vec![(1, "weather forecast today"), (2, "cookie settings")];
        let mut index = TfIdfIndex::build(&nodes);

        // Nod 2 ändras från "cookie settings" till "population data count"
        index.update_node(2, "cookie settings", "population data count");

        // Ny query borde hitta nod 2 med "population"
        let results = index.query("population data", 5);
        assert!(
            results.iter().any(|(id, _)| *id == 2),
            "Nod 2 borde hittas efter uppdatering"
        );

        // Gamla termer borde vara borta
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

        // "cookie" var unik för nod 2 → borde inte hittas
        let results = index.query("cookie settings", 5);
        assert!(
            results.iter().all(|(id, _)| *id != 2),
            "Borttagen nod borde inte hittas"
        );

        // Nod 1 borde fortfarande vara kvar ("statistics" är unik för nod 1)
        let results = index.query("statistics", 5);
        assert!(
            results.iter().any(|(id, _)| *id == 1),
            "Nod 1 borde fortfarande hittas"
        );
    }
}
