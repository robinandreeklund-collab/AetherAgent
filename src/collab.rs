/// Cross-Agent Semantic Diffing – Fas 9d
///
/// Delar semantiska deltan mellan flera agenter som arbetar
/// på samma sajt. Minskar redundant parsing och token-kostnad.
///
/// Pipeline:
/// 1. Agenter registrerar sig med sessions-ID + mål
/// 2. SharedDiffStore cachar senaste delta per URL
/// 3. Agenter kan publicera och prenumerera på deltan
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::SemanticDelta;

// ─── Types ──────────────────────────────────────────────────────────────────

/// En registrerad agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    /// Unikt agent-ID
    pub agent_id: String,
    /// Agentens mål
    pub goal: String,
    /// Registreringstid (ms sedan epoch)
    pub registered_at_ms: u64,
    /// Senaste aktivitet (ms sedan epoch)
    pub last_active_ms: u64,
    /// Antal publicerade deltan
    pub publish_count: u32,
    /// Antal konsumerade deltan
    pub consume_count: u32,
}

/// En cachad delta i butiken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDelta {
    /// URL som deltan gäller
    pub url: String,
    /// Publicerande agent
    pub publisher_agent_id: String,
    /// Själva deltan
    pub delta: SemanticDelta,
    /// Tidsstämpel (ms sedan epoch)
    pub timestamp_ms: u64,
    /// Versionsnummer (ökar vid varje uppdatering)
    pub version: u32,
}

/// Delad diff-butik – stateless, skickas som JSON
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedDiffStore {
    /// Registrerade agenter
    pub agents: Vec<AgentRegistration>,
    /// Cachade deltan per URL (senaste version)
    pub deltas: HashMap<String, CachedDelta>,
    /// Konsumtionslogg: agent_id → lista av konsumerade URL:er
    pub consumption_log: HashMap<String, Vec<String>>,
}

/// Resultat av att hämta deltan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFetchResult {
    /// Hittade deltan
    pub deltas: Vec<CachedDelta>,
    /// Antal besparade parsningar (ungefärligt)
    pub saved_parse_count: u32,
    /// Sammanfattning
    pub summary: String,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl SharedDiffStore {
    /// Skapa ny tom butik
    pub fn new() -> Self {
        Self::default()
    }

    /// Registrera en agent
    pub fn register_agent(&mut self, agent_id: &str, goal: &str, timestamp_ms: u64) {
        // Uppdatera om agenten redan finns
        if let Some(existing) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            existing.goal = goal.to_string();
            existing.last_active_ms = timestamp_ms;
            return;
        }

        self.agents.push(AgentRegistration {
            agent_id: agent_id.to_string(),
            goal: goal.to_string(),
            registered_at_ms: timestamp_ms,
            last_active_ms: timestamp_ms,
            publish_count: 0,
            consume_count: 0,
        });
    }

    /// Publicera en delta
    pub fn publish_delta(
        &mut self,
        agent_id: &str,
        url: &str,
        delta: SemanticDelta,
        timestamp_ms: u64,
    ) {
        let version = self.deltas.get(url).map(|d| d.version + 1).unwrap_or(1);

        self.deltas.insert(
            url.to_string(),
            CachedDelta {
                url: url.to_string(),
                publisher_agent_id: agent_id.to_string(),
                delta,
                timestamp_ms,
                version,
            },
        );

        // Uppdatera agentens publish_count
        if let Some(agent) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.publish_count += 1;
            agent.last_active_ms = timestamp_ms;
        }
    }

    /// Hämta alla nya deltan för en agent (som den inte redan konsumerat)
    pub fn fetch_deltas(&mut self, agent_id: &str) -> DeltaFetchResult {
        let consumed = self
            .consumption_log
            .entry(agent_id.to_string())
            .or_default();

        let mut new_deltas: Vec<CachedDelta> = self
            .deltas
            .values()
            .filter(|d| {
                // Skicka inte tillbaka agentens egna deltan
                d.publisher_agent_id != agent_id
                // Skicka inte deltan som redan konsumerats
                && !consumed.contains(&d.url)
            })
            .cloned()
            .collect();

        // Markera som konsumerade
        for delta in &new_deltas {
            consumed.push(delta.url.clone());
        }

        // Uppdatera agentens consume_count
        let count = new_deltas.len() as u32;
        if let Some(agent) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.consume_count += count;
        }

        new_deltas.sort_by(|a, b| a.timestamp_ms.cmp(&b.timestamp_ms));

        let saved = new_deltas.len() as u32;
        let summary = if new_deltas.is_empty() {
            "Inga nya deltan tillgängliga".to_string()
        } else {
            format!(
                "{} nya deltan från {} andra agenter",
                saved,
                new_deltas
                    .iter()
                    .map(|d| d.publisher_agent_id.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            )
        };

        DeltaFetchResult {
            deltas: new_deltas,
            saved_parse_count: saved,
            summary,
        }
    }

    /// Hämta delta för en specifik URL
    pub fn get_delta_for_url(&self, url: &str) -> Option<&CachedDelta> {
        self.deltas.get(url)
    }

    /// Ta bort inaktiva agenter (äldre än max_age_ms)
    pub fn cleanup_inactive(&mut self, now_ms: u64, max_age_ms: u64) {
        self.agents
            .retain(|a| now_ms - a.last_active_ms < max_age_ms);
    }

    /// Statistik
    pub fn stats(&self) -> CollabStats {
        CollabStats {
            active_agents: self.agents.len() as u32,
            cached_deltas: self.deltas.len() as u32,
            total_publishes: self.agents.iter().map(|a| a.publish_count).sum(),
            total_consumes: self.agents.iter().map(|a| a.consume_count).sum(),
        }
    }

    /// Serialisera till JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid collab store: {}", e))
    }
}

/// Sammanfattande statistik
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabStats {
    pub active_agents: u32,
    pub cached_deltas: u32,
    pub total_publishes: u32,
    pub total_consumes: u32,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChangeType, NodeChange, SemanticDelta};

    fn make_delta(url: &str) -> SemanticDelta {
        SemanticDelta {
            url: url.to_string(),
            goal: "test".to_string(),
            total_nodes_before: 10,
            total_nodes_after: 11,
            changes: vec![NodeChange {
                node_id: 1,
                change_type: ChangeType::Added,
                role: "button".to_string(),
                label: "New Button".to_string(),
                changes: vec![],
            }],
            token_savings_ratio: 0.9,
            summary: "1 added".to_string(),
            diff_time_ms: 0,
        }
    }

    #[test]
    fn test_empty_store() {
        let store = SharedDiffStore::new();
        assert!(store.agents.is_empty());
        assert!(store.deltas.is_empty());
        let stats = store.stats();
        assert_eq!(stats.active_agents, 0);
    }

    #[test]
    fn test_register_agent() {
        let mut store = SharedDiffStore::new();
        store.register_agent("agent-1", "buy shoes", 1000);
        assert_eq!(store.agents.len(), 1);
        assert_eq!(store.agents[0].agent_id, "agent-1");
        assert_eq!(store.agents[0].goal, "buy shoes");
    }

    #[test]
    fn test_register_duplicate_updates() {
        let mut store = SharedDiffStore::new();
        store.register_agent("agent-1", "buy shoes", 1000);
        store.register_agent("agent-1", "buy jackets", 2000);
        assert_eq!(store.agents.len(), 1, "Borde inte duplicera agenter");
        assert_eq!(store.agents[0].goal, "buy jackets");
        assert_eq!(store.agents[0].last_active_ms, 2000);
    }

    #[test]
    fn test_publish_and_fetch() {
        let mut store = SharedDiffStore::new();
        store.register_agent("agent-1", "buy", 1000);
        store.register_agent("agent-2", "buy", 1000);

        let delta = make_delta("https://shop.se");
        store.publish_delta("agent-1", "https://shop.se", delta, 2000);

        // agent-2 borde kunna hämta deltan
        let result = store.fetch_deltas("agent-2");
        assert_eq!(result.deltas.len(), 1, "agent-2 borde få 1 delta");
        assert_eq!(result.deltas[0].url, "https://shop.se");
        assert_eq!(result.saved_parse_count, 1);

        // agent-1 borde INTE få sin egen delta
        let result_1 = store.fetch_deltas("agent-1");
        assert!(
            result_1.deltas.is_empty(),
            "agent-1 borde inte få sin egen delta"
        );
    }

    #[test]
    fn test_fetch_only_new() {
        let mut store = SharedDiffStore::new();
        store.register_agent("a", "test", 1000);
        store.register_agent("b", "test", 1000);

        store.publish_delta(
            "a",
            "https://page1.se",
            make_delta("https://page1.se"),
            2000,
        );
        let _ = store.fetch_deltas("b"); // Konsumera första

        store.publish_delta(
            "a",
            "https://page2.se",
            make_delta("https://page2.se"),
            3000,
        );
        let result = store.fetch_deltas("b");
        assert_eq!(
            result.deltas.len(),
            1,
            "Borde bara få nya (ej redan konsumerade)"
        );
        assert_eq!(result.deltas[0].url, "https://page2.se");
    }

    #[test]
    fn test_version_increments() {
        let mut store = SharedDiffStore::new();
        store.register_agent("a", "test", 1000);

        store.publish_delta("a", "https://shop.se", make_delta("https://shop.se"), 2000);
        assert_eq!(store.deltas["https://shop.se"].version, 1);

        store.publish_delta("a", "https://shop.se", make_delta("https://shop.se"), 3000);
        assert_eq!(store.deltas["https://shop.se"].version, 2);
    }

    #[test]
    fn test_cleanup_inactive() {
        let mut store = SharedDiffStore::new();
        store.register_agent("old", "test", 1000);
        store.register_agent("new", "test", 5000);

        store.cleanup_inactive(6000, 2000);
        assert_eq!(store.agents.len(), 1, "Borde ta bort inaktiv agent");
        assert_eq!(store.agents[0].agent_id, "new");
    }

    #[test]
    fn test_stats() {
        let mut store = SharedDiffStore::new();
        store.register_agent("a", "test", 1000);
        store.register_agent("b", "test", 1000);
        store.publish_delta("a", "https://shop.se", make_delta("https://shop.se"), 2000);

        let stats = store.stats();
        assert_eq!(stats.active_agents, 2);
        assert_eq!(stats.cached_deltas, 1);
        assert_eq!(stats.total_publishes, 1);
    }

    #[test]
    fn test_get_delta_for_url() {
        let mut store = SharedDiffStore::new();
        store.register_agent("a", "test", 1000);
        store.publish_delta("a", "https://shop.se", make_delta("https://shop.se"), 2000);

        let delta = store.get_delta_for_url("https://shop.se");
        assert!(delta.is_some(), "Borde hitta delta för URL");

        let none = store.get_delta_for_url("https://nonexistent.se");
        assert!(none.is_none(), "Borde inte hitta delta för okänd URL");
    }

    #[test]
    fn test_serialization() {
        let mut store = SharedDiffStore::new();
        store.register_agent("a", "test", 1000);
        store.publish_delta("a", "https://shop.se", make_delta("https://shop.se"), 2000);

        let json = store.to_json();
        let restored = SharedDiffStore::from_json(&json).expect("Borde deserialisera");
        assert_eq!(restored.agents.len(), 1);
        assert_eq!(restored.deltas.len(), 1);
    }
}
