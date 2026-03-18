//! Multi-page Workflow Orchestrator – Fas 14
//!
//! Stateful workflow-motor som automatiskt orkestrerar multi-steg navigering:
//! - Automatisk navigation efter find_and_click (hämta nästa sida, fortsätt planen)
//! - Rollback/retry vid formulärvalideringsfel
//! - Cross-page temporal memory + semantic diff över navigeringar
//! - Integration med SessionManager för autentiserade workflows
//!
//! Designad för WASM: serialiserbar via JSON, stateless över gränsen.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::compiler::{ActionPlan, ActionType, GoalStatus};
use crate::session::SessionManager;
use crate::temporal::TemporalMemory;
use crate::types::{ClickResult, ExtractDataResult, FillFormResult, SemanticTree, WorkflowMemory};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Konfiguration för orkestrering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Max antal retries per steg
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Max antal sidor att navigera (skydd mot oändliga loopar)
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
    /// Aktivera cross-page temporal memory
    #[serde(default = "default_true")]
    pub enable_temporal_tracking: bool,
    /// Aktivera automatisk navigation efter click
    #[serde(default = "default_true")]
    pub auto_navigate: bool,
    /// Timeout per steg i ms (0 = inget timeout)
    #[serde(default)]
    pub step_timeout_ms: u64,
}

fn default_max_retries() -> u32 {
    3
}
fn default_max_pages() -> u32 {
    20
}
fn default_true() -> bool {
    true
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        OrchestratorConfig {
            max_retries: default_max_retries(),
            max_pages: default_max_pages(),
            enable_temporal_tracking: true,
            auto_navigate: true,
            step_timeout_ms: 0,
        }
    }
}

/// Status för orkestreringsmotorn
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum WorkflowStatus {
    /// Väntar på att starta
    #[default]
    NotStarted,
    /// Aktivt körande
    Running,
    /// Väntar på att hosten ska tillhandahålla HTML (efter navigation)
    AwaitingPage,
    /// Väntar på att hosten ska utföra en action (click, fill, etc.)
    AwaitingAction,
    /// Alla steg klara
    Completed,
    /// Misslyckades (max retries, max pages, etc.)
    Failed,
}

/// Orkestreringsmotor — håller hela workflow-tillståndet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOrchestrator {
    /// Mål
    pub goal: String,
    /// Handlingsplan (kompilerad från goal)
    pub plan: ActionPlan,
    /// Konfiguration
    pub config: OrchestratorConfig,
    /// Nuvarande status
    pub status: WorkflowStatus,
    /// Genomförda steg (index)
    pub completed_steps: Vec<u32>,
    /// Misslyckade steg med retry-count
    pub failed_steps: HashMap<u32, u32>,
    /// Nuvarande URL
    pub current_url: String,
    /// Sidhistorik: URL → SemanticTree-sammanfattning
    pub page_history: Vec<PageVisit>,
    /// Extraherad data (ackumulerad över alla sidor)
    pub extracted_data: HashMap<String, String>,
    /// Temporal memory (cross-page)
    #[serde(default)]
    pub temporal_memory: TemporalMemory,
    /// Sessionshanterare
    #[serde(default)]
    pub session: SessionManager,
    /// Workflow memory (steg-historik)
    #[serde(default)]
    pub workflow_memory: WorkflowMemory,
    /// Nuvarande steg-index (det vi försöker utföra)
    #[serde(default)]
    pub current_step_index: Option<u32>,
    /// Antal sidor besökta (för max_pages-skydd)
    #[serde(default)]
    pub pages_visited: u32,
}

/// En besökt sida i workflow-historiken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageVisit {
    /// URL som besöktes
    pub url: String,
    /// Sidtitel
    pub title: String,
    /// Steg-index som utlöste besöket
    pub triggered_by_step: Option<u32>,
    /// Tidsstämpel
    pub timestamp_ms: u64,
    /// Antal noder i trädet
    pub node_count: u32,
}

/// Nästa action som hosten ska utföra
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextAction {
    /// Typ av action
    pub action_type: StepAction,
    /// Beskrivning
    pub description: String,
    /// Steg-index i planen
    pub step_index: u32,
    /// URL att navigera till (för Navigate/FetchPage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Label att klicka på (för Click)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    /// Fält att fylla i (för Fill)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_fields: Option<HashMap<String, String>>,
    /// Nycklar att extrahera (för Extract)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_keys: Option<Vec<String>>,
    /// Session headers att inkludera i request
    #[serde(default)]
    pub session_headers: HashMap<String, String>,
    /// Cookie header att inkludera
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookie_header: Option<String>,
}

/// Typ av steg-action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepAction {
    /// Hämta en sida (hosten gör HTTP GET)
    FetchPage,
    /// Klicka på ett element
    Click,
    /// Fyll i formulär
    FillForm,
    /// Extrahera data
    ExtractData,
    /// Vänta (hosten väntar angivet antal ms)
    Wait,
    /// Verifiera tillstånd (ingen action, bara kontrollera trädet)
    Verify,
}

/// Resultat av att mata in en sida/action i orkestratorn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Uppdaterad orkestrator (serialiserad)
    pub orchestrator_json: String,
    /// Nästa action (om workflow inte är klar)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<NextAction>,
    /// Status
    pub status: WorkflowStatus,
    /// Sammanfattning av vad som hände
    pub summary: String,
    /// Extraherad data hittills
    pub extracted_data: HashMap<String, String>,
    /// Antal steg klara / totalt
    pub progress: String,
    /// Varningar
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl WorkflowOrchestrator {
    /// Skapa ny orkestrator från ett mål
    ///
    /// Kompilerar målet till en ActionPlan och initialiserar workflow-state.
    pub fn new(goal: &str, start_url: &str, config: OrchestratorConfig) -> Self {
        let plan = crate::compiler::compile_goal(goal);

        WorkflowOrchestrator {
            goal: goal.to_string(),
            plan,
            config,
            status: WorkflowStatus::NotStarted,
            completed_steps: Vec::new(),
            failed_steps: HashMap::new(),
            current_url: start_url.to_string(),
            page_history: Vec::new(),
            extracted_data: HashMap::new(),
            temporal_memory: TemporalMemory::default(),
            session: SessionManager::new(),
            workflow_memory: WorkflowMemory::new(),
            current_step_index: None,
            pages_visited: 0,
        }
    }

    /// Starta workflow — returnerar första action
    pub fn start(&mut self, timestamp_ms: u64) -> StepResult {
        self.status = WorkflowStatus::Running;

        // Första steget: navigera till start-URL
        let first_action = self.determine_next_action(timestamp_ms);

        let status = if first_action.is_some() {
            WorkflowStatus::AwaitingAction
        } else {
            WorkflowStatus::Completed
        };
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: first_action,
            status,
            summary: format!("Workflow startat: {}", self.goal),
            extracted_data: self.extracted_data.clone(),
            progress: format!("0/{}", self.plan.total_steps),
            warnings: vec![],
        }
    }

    /// Mata in en hämtad sida (efter FetchPage-action)
    pub fn provide_page(&mut self, html: &str, url: &str, timestamp_ms: u64) -> StepResult {
        // Kolla max pages
        self.pages_visited += 1;
        if self.pages_visited > self.config.max_pages {
            self.status = WorkflowStatus::Failed;
            return StepResult {
                orchestrator_json: self.to_json(),
                next_action: None,
                status: WorkflowStatus::Failed,
                summary: format!(
                    "Avbrutet: max antal sidor ({}) uppnått",
                    self.config.max_pages
                ),
                extracted_data: self.extracted_data.clone(),
                progress: self.progress_string(),
                warnings: vec!["Max pages exceeded".to_string()],
            };
        }

        self.current_url = url.to_string();

        // Bygg semantiskt träd
        let tree = lib_build_tree(html, &self.goal, url);

        // Registrera sidbesök
        self.page_history.push(PageVisit {
            url: url.to_string(),
            title: tree.title.clone(),
            triggered_by_step: self.current_step_index,
            timestamp_ms,
            node_count: count_nodes(&tree.nodes) as u32,
        });

        // Temporal tracking (cross-page)
        if self.config.enable_temporal_tracking {
            let tree_json = serde_json::to_string(&tree).unwrap_or_default();
            self.temporal_memory
                .add_snapshot(&tree, &tree_json, timestamp_ms);
        }

        // Om vi hade ett Navigate-steg som väntade, markera det som klart
        if let Some(step_idx) = self.current_step_index {
            if let Some(sg) = self.plan.sub_goals.get(step_idx as usize) {
                if sg.action_type == ActionType::Navigate {
                    self.complete_step(step_idx, url, "navigate", timestamp_ms);
                }
            }
        }

        // Bestäm nästa action baserat på aktuellt träd
        let next = self.determine_next_action(timestamp_ms);
        let status = self.determine_status(&next);
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: next,
            status,
            summary: format!(
                "Sida laddad: {} ({} noder)",
                tree.title,
                count_nodes(&tree.nodes)
            ),
            extracted_data: self.extracted_data.clone(),
            progress: self.progress_string(),
            warnings: vec![],
        }
    }

    /// Rapportera resultat av ett Click-steg
    pub fn report_click_result(&mut self, result: &ClickResult, timestamp_ms: u64) -> StepResult {
        let mut warnings = vec![];

        if let Some(step_idx) = self.current_step_index {
            if result.found {
                self.complete_step(step_idx, &self.current_url.clone(), "click", timestamp_ms);

                // Om click-resultatet har en href → auto-navigate
                if self.config.auto_navigate {
                    // Kolla om det finns ett value (href) i click-resultat
                    // Click returnerar selector_hint, inte href direkt,
                    // men hosten kan följa länken
                }
            } else {
                warnings.push("Inget klickbart element hittades".to_string());
                self.retry_or_fail_step(step_idx, &mut warnings);
            }
        }

        let next = self.determine_next_action(timestamp_ms);
        let status = self.determine_status(&next);
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: next,
            status,
            summary: if result.found {
                format!("Klickade på: {} ({})", result.label, result.role)
            } else {
                "Kunde inte hitta element att klicka på".to_string()
            },
            extracted_data: self.extracted_data.clone(),
            progress: self.progress_string(),
            warnings,
        }
    }

    /// Rapportera resultat av ett Fill-steg
    pub fn report_fill_result(&mut self, result: &FillFormResult, timestamp_ms: u64) -> StepResult {
        let mut warnings = vec![];

        if let Some(step_idx) = self.current_step_index {
            if result.unmapped_keys.is_empty() {
                // Alla fält mappade — markera som klart
                self.complete_step(
                    step_idx,
                    &self.current_url.clone(),
                    "fill_form",
                    timestamp_ms,
                );
            } else if !result.mappings.is_empty() {
                // Delvis lyckat — markera som klart men varna
                self.complete_step(
                    step_idx,
                    &self.current_url.clone(),
                    "fill_form",
                    timestamp_ms,
                );
                warnings.push(format!(
                    "Omappade fält: {}",
                    result.unmapped_keys.join(", ")
                ));
            } else {
                // Helt misslyckat — retry
                warnings.push("Inga formulärfält kunde mappas".to_string());
                self.retry_or_fail_step(step_idx, &mut warnings);
            }
        }

        let next = self.determine_next_action(timestamp_ms);
        let status = self.determine_status(&next);
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: next,
            status,
            summary: format!(
                "Formulär: {} fält mappade, {} omappade",
                result.mappings.len(),
                result.unmapped_keys.len()
            ),
            extracted_data: self.extracted_data.clone(),
            progress: self.progress_string(),
            warnings,
        }
    }

    /// Rapportera resultat av ett Extract-steg
    pub fn report_extract_result(
        &mut self,
        result: &ExtractDataResult,
        timestamp_ms: u64,
    ) -> StepResult {
        let mut warnings = vec![];

        // Lagra extraherad data
        for entry in &result.entries {
            self.extracted_data
                .insert(entry.key.clone(), entry.value.clone());
        }

        if let Some(step_idx) = self.current_step_index {
            if !result.entries.is_empty() {
                self.complete_step(step_idx, &self.current_url.clone(), "extract", timestamp_ms);
            } else {
                warnings.push("Ingen data kunde extraheras".to_string());
                self.retry_or_fail_step(step_idx, &mut warnings);
            }

            if !result.missing_keys.is_empty() {
                warnings.push(format!(
                    "Saknade nycklar: {}",
                    result.missing_keys.join(", ")
                ));
            }
        }

        let next = self.determine_next_action(timestamp_ms);
        let status = self.determine_status(&next);
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: next,
            status,
            summary: format!("Extraherat: {} värden", result.entries.len()),
            extracted_data: self.extracted_data.clone(),
            progress: self.progress_string(),
            warnings,
        }
    }

    /// Rapportera att ett Verify/Wait-steg är klart
    pub fn report_step_completed(&mut self, step_index: u32, timestamp_ms: u64) -> StepResult {
        self.complete_step(
            step_index,
            &self.current_url.clone(),
            "verify",
            timestamp_ms,
        );

        let next = self.determine_next_action(timestamp_ms);
        let status = self.determine_status(&next);
        self.status = status.clone();

        StepResult {
            orchestrator_json: self.to_json(),
            next_action: next,
            status,
            summary: format!("Steg {} verifierat", step_index),
            extracted_data: self.extracted_data.clone(),
            progress: self.progress_string(),
            warnings: vec![],
        }
    }

    /// Rollback: markera ett steg som ej klart och försök igen
    pub fn rollback_step(&mut self, step_index: u32) {
        self.completed_steps.retain(|&s| s != step_index);
        if let Some(sg) = self.plan.sub_goals.get_mut(step_index as usize) {
            sg.status = GoalStatus::Ready;
        }
    }

    // ── Interna hjälpmetoder ──

    /// Markera steg som klart
    fn complete_step(&mut self, step_idx: u32, url: &str, action: &str, timestamp_ms: u64) {
        if !self.completed_steps.contains(&step_idx) {
            self.completed_steps.push(step_idx);
        }

        // Uppdatera plan-status
        if let Some(sg) = self.plan.sub_goals.get_mut(step_idx as usize) {
            sg.status = GoalStatus::Completed;
        }

        // Uppdatera beroende steg: Pending → Ready
        self.update_ready_steps();

        // Registrera i workflow memory
        let desc = self
            .plan
            .sub_goals
            .get(step_idx as usize)
            .map(|sg| sg.description.clone())
            .unwrap_or_default();
        self.workflow_memory
            .add_step(action, url, &self.goal, &desc, timestamp_ms);
    }

    /// Uppdatera Ready-status för beroende steg
    fn update_ready_steps(&mut self) {
        for i in 0..self.plan.sub_goals.len() {
            let deps_met = self.plan.sub_goals[i]
                .depends_on
                .iter()
                .all(|&dep| self.completed_steps.contains(&dep));
            if deps_met && self.plan.sub_goals[i].status == GoalStatus::Pending {
                self.plan.sub_goals[i].status = GoalStatus::Ready;
            }
        }
    }

    /// Försök igen eller markera steg som misslyckat
    fn retry_or_fail_step(&mut self, step_idx: u32, warnings: &mut Vec<String>) {
        let retries = self.failed_steps.entry(step_idx).or_insert(0);
        *retries += 1;

        if *retries >= self.config.max_retries {
            if let Some(sg) = self.plan.sub_goals.get_mut(step_idx as usize) {
                sg.status = GoalStatus::Failed;
            }
            warnings.push(format!(
                "Steg {} misslyckades efter {} försök",
                step_idx, self.config.max_retries
            ));
        }
        // Om retries < max, steget förblir Ready och provas igen
    }

    /// Bestäm nästa action baserat på aktuell plan-status
    fn determine_next_action(&mut self, now_ms: u64) -> Option<NextAction> {
        // Kolla om token behöver refresh
        if self.session.needs_token_refresh(now_ms) {
            self.session.mark_token_expired();
        }

        // Hitta första Ready-steg som inte är Failed
        let next_step = self.plan.sub_goals.iter().find(|sg| {
            sg.status == GoalStatus::Ready
                && self
                    .failed_steps
                    .get(&sg.index)
                    .is_none_or(|&r| r < self.config.max_retries)
        })?;

        self.current_step_index = Some(next_step.index);

        // Bygg auth headers
        let session_headers = self.session.get_auth_headers(now_ms);
        let cookie_header = extract_domain(&self.current_url)
            .and_then(|domain| self.session.get_cookie_header(&domain, "/", now_ms));

        let action = match next_step.action_type {
            ActionType::Navigate => NextAction {
                action_type: StepAction::FetchPage,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: Some(self.current_url.clone()),
                target_label: None,
                fill_fields: None,
                extract_keys: None,
                session_headers,
                cookie_header,
            },
            ActionType::Click => NextAction {
                action_type: StepAction::Click,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: None,
                target_label: extract_target_from_description(&next_step.description),
                fill_fields: None,
                extract_keys: None,
                session_headers,
                cookie_header,
            },
            ActionType::Fill => NextAction {
                action_type: StepAction::FillForm,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: None,
                target_label: None,
                fill_fields: Some(HashMap::new()), // Hosten fyller i fälten
                extract_keys: None,
                session_headers,
                cookie_header,
            },
            ActionType::Extract => NextAction {
                action_type: StepAction::ExtractData,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: None,
                target_label: None,
                fill_fields: None,
                extract_keys: Some(extract_keys_from_description(&next_step.description)),
                session_headers,
                cookie_header,
            },
            ActionType::Wait => NextAction {
                action_type: StepAction::Wait,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: None,
                target_label: None,
                fill_fields: None,
                extract_keys: None,
                session_headers: HashMap::new(),
                cookie_header: None,
            },
            ActionType::Verify => NextAction {
                action_type: StepAction::Verify,
                description: next_step.description.clone(),
                step_index: next_step.index,
                url: None,
                target_label: None,
                fill_fields: None,
                extract_keys: None,
                session_headers: HashMap::new(),
                cookie_header: None,
            },
        };

        Some(action)
    }

    /// Bestäm workflow-status baserat på nästa action
    fn determine_status(&self, next_action: &Option<NextAction>) -> WorkflowStatus {
        // Kolla om alla steg är klara
        let all_completed = self
            .plan
            .sub_goals
            .iter()
            .all(|sg| sg.status == GoalStatus::Completed);
        if all_completed {
            return WorkflowStatus::Completed;
        }

        // Kolla om alla icke-klara steg är Failed
        let all_failed_or_done = self
            .plan
            .sub_goals
            .iter()
            .all(|sg| sg.status == GoalStatus::Completed || sg.status == GoalStatus::Failed);
        if all_failed_or_done {
            return WorkflowStatus::Failed;
        }

        match next_action {
            Some(action) => match action.action_type {
                StepAction::FetchPage => WorkflowStatus::AwaitingPage,
                _ => WorkflowStatus::AwaitingAction,
            },
            None => WorkflowStatus::Running,
        }
    }

    /// Bygg progress-sträng
    fn progress_string(&self) -> String {
        format!("{}/{}", self.completed_steps.len(), self.plan.total_steps)
    }

    /// Serialisera till JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid orchestrator JSON: {}", e))
    }
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Räkna noder rekursivt
fn count_nodes(nodes: &[crate::types::SemanticNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// Extrahera target-label från en steg-beskrivning
///
/// Letar efter text inom citattecken, t.ex. "Klicka på 'Köp nu'" → "Köp nu"
fn extract_target_from_description(description: &str) -> Option<String> {
    // Sök efter text inom enkla citattecken
    if let Some(start) = description.find('\'') {
        if let Some(end) = description[start + 1..].find('\'') {
            let target = &description[start + 1..start + 1 + end];
            if !target.is_empty() {
                return Some(target.to_string());
            }
        }
    }
    // Fallback: sista substantiella ord
    None
}

/// Extrahera nycklar från en Extract-beskrivning
///
/// Letar efter kommaseparerade nyckelord, t.ex. "Extrahera pris och produktinfo" → ["pris", "produktinfo"]
fn extract_keys_from_description(description: &str) -> Vec<String> {
    let lower = description.to_lowercase();
    let mut keys = Vec::new();

    // Vanliga datanycklar att extrahera
    let common_keys = [
        "pris",
        "price",
        "namn",
        "name",
        "titel",
        "title",
        "beskrivning",
        "description",
        "datum",
        "date",
        "version",
        "status",
        "email",
        "telefon",
        "phone",
        "adress",
        "address",
        "produktinfo",
        "product",
    ];

    for key in &common_keys {
        if lower.contains(key) {
            keys.push(key.to_string());
        }
    }

    // Om inga nycklar hittades, returnera generisk
    if keys.is_empty() {
        keys.push("data".to_string());
    }

    keys
}

/// Extrahera domän från URL
fn extract_domain(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let domain = without_scheme.split('/').next()?;
    let domain = domain.split(':').next()?;
    if domain.is_empty() {
        None
    } else {
        Some(domain.to_string())
    }
}

/// Publik build_tree wrapper (lib.rs intern funktion exponerad för orkestatorn)
pub(crate) fn lib_build_tree(html: &str, goal: &str, url: &str) -> SemanticTree {
    let dom = crate::parser::parse_html(html);
    let title = crate::semantic::extract_title(&dom);
    let mut builder = crate::semantic::SemanticBuilder::new(goal);
    builder.build(&dom, url, &title)
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_new() {
        let orch = WorkflowOrchestrator::new(
            "köp billigaste flyg",
            "https://flights.se",
            OrchestratorConfig::default(),
        );
        assert_eq!(orch.goal, "köp billigaste flyg");
        assert_eq!(orch.current_url, "https://flights.se");
        assert_eq!(orch.status, WorkflowStatus::NotStarted);
        assert!(orch.completed_steps.is_empty());
        assert!(orch.plan.total_steps > 0, "Borde ha kompilerad plan");
    }

    #[test]
    fn test_orchestrator_start() {
        let mut orch = WorkflowOrchestrator::new(
            "sök flyg",
            "https://flights.se",
            OrchestratorConfig::default(),
        );
        let result = orch.start(1000);

        assert!(
            result.next_action.is_some(),
            "Borde ha nästa action efter start"
        );
        assert_ne!(
            result.status,
            WorkflowStatus::NotStarted,
            "Borde inte vara NotStarted efter start"
        );
    }

    #[test]
    fn test_orchestrator_provide_page() {
        let mut orch = WorkflowOrchestrator::new(
            "hitta pris",
            "https://shop.se",
            OrchestratorConfig::default(),
        );
        orch.start(1000);

        let html = r##"<html><head><title>Shop</title></head><body>
            <h1>Produkter</h1>
            <button>Köp nu</button>
            <a href="/checkout">Gå till kassa</a>
        </body></html>"##;

        let result = orch.provide_page(html, "https://shop.se", 2000);
        assert!(!result.summary.is_empty(), "Borde ha sammanfattning");
        assert_eq!(orch.pages_visited, 1);
        assert_eq!(orch.page_history.len(), 1);
    }

    #[test]
    fn test_orchestrator_max_pages_protection() {
        let config = OrchestratorConfig {
            max_pages: 2,
            ..Default::default()
        };
        let mut orch = WorkflowOrchestrator::new("browse", "https://test.se", config);
        orch.start(1000);

        let html = "<html><body><p>Page</p></body></html>";
        orch.provide_page(html, "https://test.se/1", 2000);
        orch.provide_page(html, "https://test.se/2", 3000);
        let result = orch.provide_page(html, "https://test.se/3", 4000);

        assert_eq!(
            result.status,
            WorkflowStatus::Failed,
            "Borde misslyckas vid max pages"
        );
    }

    #[test]
    fn test_orchestrator_click_result_success() {
        let mut orch = WorkflowOrchestrator::new(
            "köp produkt",
            "https://shop.se",
            OrchestratorConfig::default(),
        );
        orch.start(1000);

        // Simulera att vi är på ett Click-steg
        // Hitta första Click-steg
        let click_info: Option<(u32, Vec<u32>)> = orch
            .plan
            .sub_goals
            .iter()
            .find(|sg| sg.action_type == ActionType::Click)
            .map(|sg| (sg.index, sg.depends_on.clone()));
        if let Some((step_idx, deps)) = click_info {
            orch.current_step_index = Some(step_idx);
            // Markera Navigate som klart om det är en dependency
            for dep in deps {
                orch.complete_step(dep, "https://shop.se", "navigate", 1500);
            }

            let click_result = ClickResult {
                found: true,
                node_id: 1,
                role: "button".to_string(),
                label: "Köp".to_string(),
                action: "click".to_string(),
                relevance: 0.9,
                selector_hint: "button:nth-of-type(1)".to_string(),
                trust: crate::types::TrustLevel::Structural,
                injection_warnings: vec![],
                parse_time_ms: 5,
            };

            let result = orch.report_click_result(&click_result, 2000);
            assert!(
                result.summary.contains("Klickade"),
                "Borde rapportera klick: {}",
                result.summary
            );
        }
    }

    #[test]
    fn test_orchestrator_click_result_failure_retries() {
        let config = OrchestratorConfig {
            max_retries: 2,
            ..Default::default()
        };
        let mut orch = WorkflowOrchestrator::new("klicka", "https://test.se", config);
        orch.start(1000);

        // Hitta click-steg och gör det Ready
        let click_step_idx = orch
            .plan
            .sub_goals
            .iter()
            .find(|sg| sg.action_type == ActionType::Click)
            .map(|sg| sg.index);

        if let Some(idx) = click_step_idx {
            orch.current_step_index = Some(idx);
            // Säkerställ att steget är Ready
            if let Some(sg) = orch.plan.sub_goals.get_mut(idx as usize) {
                sg.status = GoalStatus::Ready;
            }

            let fail_result = ClickResult::not_found(vec![], 5);

            // Första failure
            let r = orch.report_click_result(&fail_result, 2000);
            assert!(!r.warnings.is_empty(), "Borde varna vid misslyckad click");

            // Sätt current_step_index igen
            orch.current_step_index = Some(idx);

            // Andra failure → borde markera som Failed
            let r = orch.report_click_result(&fail_result, 3000);
            let step_status = orch.plan.sub_goals.get(idx as usize).map(|sg| &sg.status);
            assert_eq!(
                step_status,
                Some(&GoalStatus::Failed),
                "Borde vara Failed efter max retries: {:?}",
                r.warnings
            );
        }
    }

    #[test]
    fn test_orchestrator_extract_data() {
        let mut orch = WorkflowOrchestrator::new(
            "extrahera priser",
            "https://shop.se",
            OrchestratorConfig::default(),
        );
        orch.start(1000);

        // Hitta Extract-steg
        let extract_step = orch
            .plan
            .sub_goals
            .iter()
            .find(|sg| sg.action_type == ActionType::Extract)
            .map(|sg| sg.index);

        if let Some(idx) = extract_step {
            orch.current_step_index = Some(idx);
            // Markera dependencies som klara
            let deps: Vec<u32> = orch.plan.sub_goals[idx as usize].depends_on.clone();
            for dep in deps {
                orch.complete_step(dep, "https://shop.se", "navigate", 1500);
            }

            let extract_result = ExtractDataResult {
                entries: vec![crate::types::ExtractedEntry {
                    key: "pris".to_string(),
                    value: "1299 kr".to_string(),
                    source_node_id: 5,
                    confidence: 0.85,
                }],
                missing_keys: vec![],
                injection_warnings: vec![],
                parse_time_ms: 3,
            };

            let result = orch.report_extract_result(&extract_result, 2000);
            assert_eq!(
                result.extracted_data.get("pris"),
                Some(&"1299 kr".to_string()),
                "Borde lagra extraherat data"
            );
        }
    }

    #[test]
    fn test_orchestrator_rollback_step() {
        let mut orch =
            WorkflowOrchestrator::new("köp", "https://shop.se", OrchestratorConfig::default());
        orch.start(1000);

        // Manuellt klara steg 0
        orch.complete_step(0, "https://shop.se", "navigate", 1500);
        assert!(orch.completed_steps.contains(&0));

        // Rollback
        orch.rollback_step(0);
        assert!(
            !orch.completed_steps.contains(&0),
            "Steg borde vara borttaget efter rollback"
        );
    }

    #[test]
    fn test_orchestrator_serialization_roundtrip() {
        let mut orch =
            WorkflowOrchestrator::new("test", "https://test.se", OrchestratorConfig::default());
        orch.start(1000);

        let json = orch.to_json();
        let restored = WorkflowOrchestrator::from_json(&json).expect("Borde deserialisera");
        assert_eq!(restored.goal, "test");
        assert_eq!(restored.current_url, "https://test.se");
    }

    #[test]
    fn test_orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.max_pages, 20);
        assert!(config.enable_temporal_tracking);
        assert!(config.auto_navigate);
    }

    #[test]
    fn test_extract_target_from_description() {
        assert_eq!(
            extract_target_from_description("Klicka på 'Köp nu'"),
            Some("Köp nu".to_string())
        );
        assert_eq!(extract_target_from_description("Klicka på knappen"), None);
    }

    #[test]
    fn test_extract_keys_from_description() {
        let keys = extract_keys_from_description("Extrahera pris och produktinfo");
        assert!(keys.contains(&"pris".to_string()));
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://shop.se/products"),
            Some("shop.se".to_string())
        );
        assert_eq!(
            extract_domain("http://localhost:3000/api"),
            Some("localhost".to_string())
        );
        assert_eq!(extract_domain("not-a-url"), None);
    }

    #[test]
    fn test_orchestrator_temporal_tracking() {
        let mut orch =
            WorkflowOrchestrator::new("test", "https://test.se", OrchestratorConfig::default());
        orch.start(1000);

        let html = "<html><body><p>Page 1</p></body></html>";
        orch.provide_page(html, "https://test.se/1", 2000);

        let html2 = "<html><body><p>Page 2 changed</p></body></html>";
        orch.provide_page(html2, "https://test.se/2", 3000);

        assert_eq!(
            orch.temporal_memory.snapshots.len(),
            2,
            "Borde ha 2 temporal snapshots"
        );
    }

    #[test]
    fn test_orchestrator_session_integration() {
        let mut orch =
            WorkflowOrchestrator::new("test", "https://test.se", OrchestratorConfig::default());

        // Konfigurera session
        orch.session
            .add_cookies_from_headers("test.se", &["session=abc123; Path=/".to_string()]);
        orch.session
            .set_oauth_token("token_123", None, 3600, 0, vec![]);

        let result = orch.start(1000);
        if let Some(action) = &result.next_action {
            assert!(
                action.session_headers.contains_key("Authorization"),
                "Borde inkludera Authorization header"
            );
        }
    }

    #[test]
    fn test_orchestrator_workflow_completed() {
        let mut orch =
            WorkflowOrchestrator::new("test", "https://test.se", OrchestratorConfig::default());
        orch.start(1000);

        // Markera alla steg som klara
        let total = orch.plan.sub_goals.len();
        for i in 0..total {
            orch.complete_step(i as u32, "https://test.se", "test", 1000 + (i as u64) * 100);
        }

        let next = orch.determine_next_action(5000);
        let status = orch.determine_status(&next);
        assert_eq!(
            status,
            WorkflowStatus::Completed,
            "Borde vara Completed när alla steg är klara"
        );
    }

    #[test]
    fn test_orchestrator_from_invalid_json() {
        let result = WorkflowOrchestrator::from_json("invalid");
        assert!(result.is_err(), "Borde ge fel för ogiltig JSON");
    }

    #[test]
    fn test_page_visit_recorded() {
        let mut orch =
            WorkflowOrchestrator::new("test", "https://test.se", OrchestratorConfig::default());
        orch.start(1000);

        let html = "<html><head><title>Test Page</title></head><body><p>Content</p></body></html>";
        orch.provide_page(html, "https://test.se", 2000);

        assert_eq!(orch.page_history.len(), 1);
        assert_eq!(orch.page_history[0].url, "https://test.se");
        assert_eq!(orch.page_history[0].title, "Test Page");
    }
}
