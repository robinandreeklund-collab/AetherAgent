/// Workflow Memory – stateless kontext mellan agent-steg
///
/// Designad för WASM: ingen global state, ingen filesystem.
/// Hosten (Python/Node) äger det serialiserade minnet och skickar det
/// fram och tillbaka över WASM-gränsen som JSON.
use crate::types::{WorkflowMemory, WorkflowStep};

impl WorkflowMemory {
    /// Lägg till ett steg i workflow-historiken
    pub fn add_step(
        &mut self,
        action: &str,
        url: &str,
        goal: &str,
        summary: &str,
        timestamp_ms: u64,
    ) {
        let step_index = self.steps.len() as u32;
        self.steps.push(WorkflowStep {
            step_index,
            action: action.to_string(),
            url: url.to_string(),
            goal: goal.to_string(),
            summary: summary.to_string(),
            timestamp_ms,
        });
    }

    /// Spara ett nyckel-värde-par i kontexten
    pub fn set_context(&mut self, key: &str, value: &str) {
        self.context.insert(key.to_string(), value.to_string());
    }

    /// Hämta ett värde från kontexten
    pub fn get_context(&self, key: &str) -> Option<&str> {
        self.context.get(key).map(|s| s.as_str())
    }

    /// Serialisera till JSON för transport över WASM-gränsen
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid workflow memory JSON: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_roundtrip() {
        let mut mem = WorkflowMemory::new();
        mem.add_step(
            "click",
            "https://shop.se",
            "köp sko",
            "Klickade på Köp-knappen",
            1000,
        );
        mem.add_step(
            "fill_form",
            "https://shop.se/checkout",
            "fyll i adress",
            "Fyllde i leveransadress",
            2000,
        );
        mem.set_context("cart_total", "1299 kr");

        let json = mem.to_json();
        let restored = WorkflowMemory::from_json(&json).expect("Borde kunna deserialisera");

        assert_eq!(restored.steps.len(), 2);
        assert_eq!(restored.steps[0].action, "click");
        assert_eq!(restored.steps[0].step_index, 0);
        assert_eq!(restored.steps[1].action, "fill_form");
        assert_eq!(restored.steps[1].step_index, 1);
        assert_eq!(restored.get_context("cart_total"), Some("1299 kr"));
    }

    #[test]
    fn test_memory_context_set_get() {
        let mut mem = WorkflowMemory::new();
        assert_eq!(mem.get_context("nonexistent"), None);

        mem.set_context("user_email", "test@test.se");
        assert_eq!(mem.get_context("user_email"), Some("test@test.se"));

        // Överskrivning
        mem.set_context("user_email", "new@test.se");
        assert_eq!(mem.get_context("user_email"), Some("new@test.se"));
    }

    #[test]
    fn test_memory_from_invalid_json() {
        let result = WorkflowMemory::from_json("this is not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_empty_roundtrip() {
        let mem = WorkflowMemory::new();
        let json = mem.to_json();
        let restored =
            WorkflowMemory::from_json(&json).expect("Tom memory borde gå att serialisera");
        assert!(restored.steps.is_empty());
        assert!(restored.context.is_empty());
    }
}
