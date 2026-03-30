/// Intent Compiler – Fas 6
///
/// Kompilerar mål till optimerade handlingsplaner.
///
/// Pipeline:
/// 1. Dekomponera ett komplext mål till delmål
/// 2. Identifiera beroenden mellan delmål
/// 3. Optimera ordningen (parallella vs sekventiella steg)
/// 4. Spekulativ prefetch: beräkna förväntade semantiska träd
/// 5. Cachelagra resultat för upprepade workflows
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::semantic::text_similarity;
use crate::types::SemanticTree;

// ─── Types ──────────────────────────────────────────────────────────────────

/// Ett delmål i en dekomponerad plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGoal {
    /// Unikt index
    pub index: u32,
    /// Beskrivning av delmålet
    pub description: String,
    /// Vilken action-typ som behövs
    pub action_type: ActionType,
    /// Beroenden: vilka andra delmål som måste vara klara först
    pub depends_on: Vec<u32>,
    /// Beräknad kostnad (relativt, 0.0–1.0)
    pub estimated_cost: f32,
    /// Status
    pub status: GoalStatus,
}

/// Action-typ för delmål
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActionType {
    /// Navigera till en URL
    Navigate,
    /// Klicka på ett element
    Click,
    /// Fyll i ett formulärfält
    Fill,
    /// Extrahera data
    Extract,
    /// Vänta på att sidan laddas
    Wait,
    /// Validera att ett tillstånd uppnåtts
    Verify,
}

/// Status för ett delmål
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalStatus {
    Pending,
    Ready,
    InProgress,
    Completed,
    Failed,
}

/// En optimerad handlingsplan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    /// Originalmålet
    pub goal: String,
    /// Alla delmål
    pub sub_goals: Vec<SubGoal>,
    /// Optimerad exekveringsordning (index i sub_goals)
    pub execution_order: Vec<Vec<u32>>,
    /// Totalt antal steg
    pub total_steps: u32,
    /// Antal parallelliserbara steg
    pub parallel_groups: u32,
    /// Beräknad total kostnad
    pub estimated_total_cost: f32,
    /// Kompileringstid
    pub compile_time_ms: u64,
}

/// Prefetch-cache: förväntade sidor som kan förberäknas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefetchEntry {
    /// URL som förväntas besökas
    pub expected_url: String,
    /// Sannolikhet (0.0–1.0) att denna sida besöks
    pub probability: f32,
    /// Förberäknat semantiskt träd (om tillgängligt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precomputed_tree: Option<SemanticTree>,
}

/// Resultat av att köra en plan mot aktuellt tillstånd
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanExecutionResult {
    /// Planen
    pub plan: ActionPlan,
    /// Nuvarande steg
    pub current_step: u32,
    /// Nästa rekommenderad action
    pub next_action: Option<RecommendedAction>,
    /// Prefetch-förslag
    pub prefetch_suggestions: Vec<PrefetchEntry>,
    /// Sammanfattning
    pub summary: String,
}

/// Rekommenderad nästa action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendedAction {
    /// Delmålets index
    pub sub_goal_index: u32,
    /// Action-typ
    pub action_type: ActionType,
    /// Beskrivning
    pub description: String,
    /// Om det är ett click, vilken label att söka
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
    /// Om det är fill, vilka fält
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_fields: Option<HashMap<String, String>>,
    /// Om det är extract, vilka nycklar
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_keys: Option<Vec<String>>,
    /// Konfidens (0.0–1.0)
    pub confidence: f32,
}

// ─── Mål-dekomponering ──────────────────────────────────────────────────────

/// Känd mål-mall med delmål
struct GoalTemplate {
    keywords: &'static [&'static str],
    sub_goals: Vec<(ActionType, &'static str, Vec<u32>)>,
}

/// Hämta mallar för kända mål-typer
///
/// Ordnade efter specificitet — mer specifika mallar först.
/// Matchning väljer den mall med högst keyword-score.
fn get_goal_templates() -> Vec<GoalTemplate> {
    vec![
        // ── Specifika domänmallar (högre prioritet) ──
        GoalTemplate {
            keywords: &["price", "pris", "cost", "kostnad", "product", "produkt"],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till produktsidan", vec![]),
                (
                    ActionType::Extract,
                    "Extrahera pris och produktinfo",
                    vec![0],
                ),
                (ActionType::Verify, "Verifiera att pris hittades", vec![1]),
            ],
        },
        GoalTemplate {
            keywords: &[
                "version",
                "release",
                "latest",
                "senaste",
                "changelog",
                "update",
            ],
            sub_goals: vec![
                (
                    ActionType::Navigate,
                    "Navigera till release/version-sida",
                    vec![],
                ),
                (
                    ActionType::Extract,
                    "Extrahera version, datum och release-info",
                    vec![0],
                ),
                (
                    ActionType::Verify,
                    "Verifiera att versionsinfo hittades",
                    vec![1],
                ),
            ],
        },
        GoalTemplate {
            keywords: &["compare", "jämför", "vs", "skillnad", "difference"],
            sub_goals: vec![
                (
                    ActionType::Navigate,
                    "Navigera till första alternativet",
                    vec![],
                ),
                (
                    ActionType::Extract,
                    "Extrahera data från alternativ 1",
                    vec![0],
                ),
                (
                    ActionType::Navigate,
                    "Navigera till andra alternativet",
                    vec![0],
                ),
                (
                    ActionType::Extract,
                    "Extrahera data från alternativ 2",
                    vec![2],
                ),
                (
                    ActionType::Verify,
                    "Jämför och sammanställ resultat",
                    vec![1, 3],
                ),
            ],
        },
        // ── Generella action-mallar ──
        GoalTemplate {
            keywords: &["köp", "buy", "purchase", "beställ", "order"],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till produktsida", vec![]),
                (ActionType::Click, "Klicka på 'Lägg i varukorg'", vec![0]),
                (ActionType::Navigate, "Gå till kassan", vec![1]),
                (ActionType::Fill, "Fyll i leveransinformation", vec![2]),
                (ActionType::Fill, "Fyll i betalningsinformation", vec![3]),
                (ActionType::Click, "Bekräfta beställning", vec![4]),
                (ActionType::Verify, "Verifiera orderbekräftelse", vec![5]),
            ],
        },
        GoalTemplate {
            keywords: &["logga in", "login", "sign in", "log in"],
            sub_goals: vec![
                (
                    ActionType::Navigate,
                    "Navigera till inloggningssida",
                    vec![],
                ),
                (ActionType::Fill, "Fyll i e-post/användarnamn", vec![0]),
                (ActionType::Fill, "Fyll i lösenord", vec![1]),
                (ActionType::Click, "Klicka på 'Logga in'", vec![1, 2]),
                (ActionType::Verify, "Verifiera inloggning lyckades", vec![3]),
            ],
        },
        GoalTemplate {
            keywords: &["sök", "search"],
            sub_goals: vec![
                (ActionType::Fill, "Skriv sökterm i sökfält", vec![]),
                (ActionType::Click, "Klicka på 'Sök'", vec![0]),
                (ActionType::Extract, "Extrahera sökresultat", vec![1]),
            ],
        },
        GoalTemplate {
            keywords: &[
                "registrera",
                "register",
                "sign up",
                "skapa konto",
                "create account",
            ],
            sub_goals: vec![
                (
                    ActionType::Navigate,
                    "Navigera till registreringssida",
                    vec![],
                ),
                (ActionType::Fill, "Fyll i namn", vec![0]),
                (ActionType::Fill, "Fyll i e-postadress", vec![0]),
                (ActionType::Fill, "Fyll i lösenord", vec![0]),
                (ActionType::Click, "Klicka på 'Registrera'", vec![1, 2, 3]),
                (ActionType::Verify, "Verifiera registrering", vec![4]),
            ],
        },
        GoalTemplate {
            keywords: &["extrahera", "extract", "scrape", "hämta data", "get data"],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till sidan", vec![]),
                (ActionType::Extract, "Extrahera begärd data", vec![0]),
                (ActionType::Verify, "Verifiera att data hämtades", vec![1]),
            ],
        },
        // ── Domänspecifika mallar (BUG-6 fix: kontextmedvetna templates) ──
        GoalTemplate {
            keywords: &[
                "kontakt", "contact", "telefon", "phone", "epost", "email", "adress", "address",
            ],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till kontaktsidan", vec![]),
                (
                    ActionType::Extract,
                    "Extrahera kontaktuppgifter (telefon, epost, adress)",
                    vec![0],
                ),
                (
                    ActionType::Verify,
                    "Verifiera att kontaktinfo hittades",
                    vec![1],
                ),
            ],
        },
        GoalTemplate {
            keywords: &[
                "analysera",
                "analyze",
                "analyse",
                "granska",
                "review",
                "overview",
                "överblick",
            ],
            sub_goals: vec![
                (
                    ActionType::Navigate,
                    "Navigera till sidan som ska analyseras",
                    vec![],
                ),
                (
                    ActionType::Extract,
                    "Extrahera struktur, navigation och nyckelinnehåll",
                    vec![0],
                ),
                (
                    ActionType::Extract,
                    "Extrahera detaljerad data (kontakt, nyheter, tjänster)",
                    vec![0],
                ),
                (
                    ActionType::Verify,
                    "Sammanställ och verifiera analysresultat",
                    vec![1, 2],
                ),
            ],
        },
        GoalTemplate {
            keywords: &["nyhet", "nyheter", "news", "artikel", "article", "blog"],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till nyhetssidan", vec![]),
                (
                    ActionType::Extract,
                    "Extrahera nyhetsrubriker, datum och sammanfattningar",
                    vec![0],
                ),
                (
                    ActionType::Verify,
                    "Verifiera att nyheter hittades",
                    vec![1],
                ),
            ],
        },
        GoalTemplate {
            keywords: &[
                "navigera", "navigate", "browse", "bläddra", "utforska", "explore",
            ],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till startsidan", vec![]),
                (
                    ActionType::Extract,
                    "Kartlägg navigationslänkar och sidstruktur",
                    vec![0],
                ),
                (
                    ActionType::Navigate,
                    "Navigera till relevant undersida",
                    vec![1],
                ),
                (
                    ActionType::Extract,
                    "Extrahera innehåll från undersidan",
                    vec![2],
                ),
            ],
        },
        // ── Breda mallar (lägst prioritet, "find/hitta" är extraktion, inte sökning) ──
        GoalTemplate {
            keywords: &["hitta", "find", "get", "hämta", "check", "kolla"],
            sub_goals: vec![
                (ActionType::Navigate, "Navigera till relevant sida", vec![]),
                (ActionType::Extract, "Extrahera efterfrågad data", vec![0]),
                (ActionType::Verify, "Verifiera att data hittades", vec![1]),
            ],
        },
    ]
}

// ─── Compiler ───────────────────────────────────────────────────────────────

/// Kompilera ett mål till en handlingsplan
pub fn compile_goal(goal: &str) -> ActionPlan {
    let lower_goal = goal.to_lowercase();
    let templates = get_goal_templates();

    // Matcha mot mallar
    // Score = (bästa keyword-match) + (antal keyword-matcher * 0.1) + specificitet
    // Mallar listade tidigare (mer specifika) får högre specificitet vid lika poäng
    let template_count = templates.len();
    let best_template = templates
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let keyword_scores: Vec<f32> = t
                .keywords
                .iter()
                .map(|kw| {
                    if lower_goal.contains(kw) {
                        1.0
                    } else {
                        // Embedding-förstärkt keyword matching
                        let word_sim = text_similarity(kw, &lower_goal) * 0.5;
                        if word_sim < 0.3 {
                            let emb_sim =
                                crate::embedding::similarity(kw, &lower_goal).unwrap_or(0.0) * 0.6;
                            word_sim.max(emb_sim)
                        } else {
                            word_sim
                        }
                    }
                })
                .collect();
            let best = keyword_scores.iter().copied().fold(0.0f32, f32::max);
            let match_count = keyword_scores.iter().filter(|&&s| s > 0.3).count();
            // Specificitet: tidigare mallar vinner vid lika poäng
            let specificity = (template_count - idx) as f32 * 0.001;
            let score = best + (match_count as f32 * 0.1) + specificity;
            (t, score)
        })
        .max_by(|(_, a), (_, b)| a.total_cmp(b));

    let sub_goals = if let Some((template, score)) = best_template {
        if score > 0.3 {
            template
                .sub_goals
                .iter()
                .enumerate()
                .map(|(i, (action_type, desc, deps))| SubGoal {
                    index: i as u32,
                    description: desc.to_string(),
                    action_type: action_type.clone(),
                    depends_on: deps.clone(),
                    estimated_cost: estimate_action_cost(action_type),
                    status: if deps.is_empty() {
                        GoalStatus::Ready
                    } else {
                        GoalStatus::Pending
                    },
                })
                .collect()
        } else {
            // Fallback: generisk plan
            build_generic_plan(goal)
        }
    } else {
        build_generic_plan(goal)
    };

    let execution_order = compute_execution_order(&sub_goals);
    let total_steps = sub_goals.len() as u32;
    let parallel_groups = execution_order
        .iter()
        .map(|g| g.len() as u32)
        .max()
        .unwrap_or(1);
    let estimated_total_cost: f32 = sub_goals.iter().map(|sg| sg.estimated_cost).sum();

    ActionPlan {
        goal: goal.to_string(),
        sub_goals,
        execution_order,
        total_steps,
        parallel_groups,
        estimated_total_cost,
        compile_time_ms: 0,
    }
}

/// Beräkna optimerad exekveringsordning (topologisk sortering med parallellisering)
fn compute_execution_order(sub_goals: &[SubGoal]) -> Vec<Vec<u32>> {
    let mut order: Vec<Vec<u32>> = Vec::new();
    let mut completed: Vec<bool> = vec![false; sub_goals.len()];
    let total = sub_goals.len();
    let mut done = 0;

    // Topologisk sortering med parallella grupper
    while done < total {
        let mut group: Vec<u32> = Vec::new();
        for sg in sub_goals {
            if completed[sg.index as usize] {
                continue;
            }
            let deps_met = sg
                .depends_on
                .iter()
                .all(|&dep| completed.get(dep as usize).copied().unwrap_or(true));
            if deps_met {
                group.push(sg.index);
            }
        }

        if group.is_empty() {
            // Cykliskt beroende eller felaktig graf – bryt ut
            break;
        }

        for &idx in &group {
            completed[idx as usize] = true;
            done += 1;
        }
        order.push(group);
    }

    order
}

/// Generisk plan för okända mål
///
/// Default: Navigate → Extract → Verify. Inte Fill → Click → Extract,
/// eftersom de flesta mål handlar om att hämta information.
fn build_generic_plan(goal: &str) -> Vec<SubGoal> {
    vec![
        SubGoal {
            index: 0,
            description: format!("Navigera till relevant sida för '{}'", goal),
            action_type: ActionType::Navigate,
            depends_on: vec![],
            estimated_cost: 0.3,
            status: GoalStatus::Ready,
        },
        SubGoal {
            index: 1,
            description: format!("Extrahera relevant data för '{}'", goal),
            action_type: ActionType::Extract,
            depends_on: vec![0],
            estimated_cost: 0.3,
            status: GoalStatus::Pending,
        },
        SubGoal {
            index: 2,
            description: format!("Verifiera att '{}' lyckades", goal),
            action_type: ActionType::Verify,
            depends_on: vec![1],
            estimated_cost: 0.2,
            status: GoalStatus::Pending,
        },
    ]
}

/// Uppskatta kostnad per action-typ
fn estimate_action_cost(action: &ActionType) -> f32 {
    match action {
        ActionType::Navigate => 0.3,
        ActionType::Click => 0.2,
        ActionType::Fill => 0.25,
        ActionType::Extract => 0.15,
        ActionType::Wait => 0.1,
        ActionType::Verify => 0.1,
    }
}

// ─── Plan-exekvering ────────────────────────────────────────────────────────

/// Exekvera plan mot aktuellt tillstånd och ge nästa rekommenderade action
pub fn execute_plan(
    plan: &ActionPlan,
    tree: &SemanticTree,
    completed_steps: &[u32],
) -> PlanExecutionResult {
    let mut updated_plan = plan.clone();

    // Markera klara steg
    for &idx in completed_steps {
        if let Some(sg) = updated_plan.sub_goals.get_mut(idx as usize) {
            sg.status = GoalStatus::Completed;
        }
    }

    // Uppdatera Ready-status
    for i in 0..updated_plan.sub_goals.len() {
        let deps_met = updated_plan.sub_goals[i].depends_on.iter().all(|&dep| {
            updated_plan
                .sub_goals
                .get(dep as usize)
                .map(|sg| sg.status == GoalStatus::Completed)
                .unwrap_or(true)
        });
        if deps_met && updated_plan.sub_goals[i].status == GoalStatus::Pending {
            updated_plan.sub_goals[i].status = GoalStatus::Ready;
        }
    }

    // Hitta nästa action
    let next_action = updated_plan
        .sub_goals
        .iter()
        .find(|sg| sg.status == GoalStatus::Ready)
        .map(|sg| build_recommended_action(sg, tree));

    let current_step = completed_steps.len() as u32;

    // Bygg prefetch-förslag
    let prefetch = build_prefetch_suggestions(&updated_plan, completed_steps);

    let completed_count = updated_plan
        .sub_goals
        .iter()
        .filter(|sg| sg.status == GoalStatus::Completed)
        .count();

    let summary = format!(
        "{}/{} steg klara, {} redo",
        completed_count,
        updated_plan.total_steps,
        updated_plan
            .sub_goals
            .iter()
            .filter(|sg| sg.status == GoalStatus::Ready)
            .count()
    );

    PlanExecutionResult {
        plan: updated_plan,
        current_step,
        next_action,
        prefetch_suggestions: prefetch,
        summary,
    }
}

/// Bygg rekommenderad action baserat på delmål och aktuellt träd
fn build_recommended_action(sub_goal: &SubGoal, tree: &SemanticTree) -> RecommendedAction {
    let all_nodes = crate::intent::flatten_nodes_pub(&tree.nodes);

    let (target_label, confidence) = match sub_goal.action_type {
        ActionType::Click => {
            // Hitta bästa matchande klickbara element
            let best = all_nodes
                .iter()
                .filter(|n| {
                    matches!(
                        n.role.as_str(),
                        "button" | "link" | "menuitem" | "cta" | "product_card"
                    )
                })
                .map(|n| (n, text_similarity(&sub_goal.description, &n.label)))
                .max_by(|(_, a), (_, b)| a.total_cmp(b));

            match best {
                Some((node, sim)) => (Some(node.label.clone()), sim.min(1.0)),
                None => (None, 0.3),
            }
        }
        ActionType::Fill => (None, 0.5),
        ActionType::Extract => (None, 0.6),
        ActionType::Navigate => (None, 0.4),
        ActionType::Wait => (None, 0.8),
        ActionType::Verify => (None, 0.5),
    };

    RecommendedAction {
        sub_goal_index: sub_goal.index,
        action_type: sub_goal.action_type.clone(),
        description: sub_goal.description.clone(),
        target_label,
        fill_fields: None,
        extract_keys: None,
        confidence,
    }
}

/// Bygg prefetch-förslag baserat på plan
fn build_prefetch_suggestions(plan: &ActionPlan, completed_steps: &[u32]) -> Vec<PrefetchEntry> {
    let mut suggestions = Vec::new();

    // Hitta kommande Navigate-steg
    for sg in &plan.sub_goals {
        if sg.status != GoalStatus::Completed
            && sg.action_type == ActionType::Navigate
            && !completed_steps.contains(&sg.index)
        {
            // Uppskatta URL från beskrivningen
            let prob = if sg
                .depends_on
                .iter()
                .all(|dep| completed_steps.contains(dep))
            {
                0.8
            } else {
                0.4
            };

            suggestions.push(PrefetchEntry {
                expected_url: format!("(predicted from: {})", sg.description),
                probability: prob,
                precomputed_tree: None,
            });
        }
    }

    suggestions
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SemanticNode, SemanticTree};

    fn make_tree(nodes: Vec<SemanticNode>) -> SemanticTree {
        SemanticTree {
            url: "https://shop.se".to_string(),
            title: "Shop".to_string(),
            goal: "test".to_string(),
            nodes,
            injection_warnings: vec![],
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
        }
    }

    #[test]
    fn test_compile_buy_goal() {
        let plan = compile_goal("köp iPhone 16 Pro");
        assert!(!plan.sub_goals.is_empty(), "Borde ha delmål");
        assert!(plan.sub_goals.len() >= 5, "Köp-plan borde ha minst 5 steg");
        assert_eq!(plan.goal, "köp iPhone 16 Pro");

        // Första steget ska vara Ready
        assert_eq!(plan.sub_goals[0].status, GoalStatus::Ready);
        // Andra steget ska vara Pending (beror på första)
        assert_eq!(plan.sub_goals[1].status, GoalStatus::Pending);
    }

    #[test]
    fn test_compile_login_goal() {
        let plan = compile_goal("logga in på min sida");
        assert!(
            plan.sub_goals.len() >= 4,
            "Login-plan borde ha minst 4 steg"
        );

        // Borde ha Fill-steg för email och lösenord
        let fill_steps: Vec<_> = plan
            .sub_goals
            .iter()
            .filter(|sg| sg.action_type == ActionType::Fill)
            .collect();
        assert!(fill_steps.len() >= 2, "Borde ha minst 2 fyll-steg");
    }

    #[test]
    fn test_compile_search_goal() {
        let plan = compile_goal("sök efter billiga flyg");
        assert!(!plan.sub_goals.is_empty());
        // Borde ha ett Extract-steg
        let extract = plan
            .sub_goals
            .iter()
            .any(|sg| sg.action_type == ActionType::Extract);
        assert!(extract, "Sök-plan borde ha Extract-steg");
    }

    #[test]
    fn test_compile_unknown_goal() {
        let plan = compile_goal("gör något ovanligt");
        assert!(
            plan.sub_goals.len() >= 3,
            "Generisk plan borde ha minst 3 steg"
        );
    }

    #[test]
    fn test_compile_kontakt_goal() {
        // BUG-6 regression: kontakt-mål borde ge specifik kontakt-plan
        let plan = compile_goal("hitta kontaktinformation för Hjo kommun");
        let has_kontakt_step = plan
            .sub_goals
            .iter()
            .any(|sg| sg.description.to_lowercase().contains("kontakt"));
        assert!(
            has_kontakt_step,
            "Kontakt-mål borde generera kontaktspecifika steg, fick: {:?}",
            plan.sub_goals
                .iter()
                .map(|sg| &sg.description)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_compile_analysera_goal() {
        let plan = compile_goal("Analysera Hjo kommuns webbplats");
        assert!(
            plan.sub_goals.len() >= 3,
            "Analys-plan borde ha minst 3 steg"
        );
        let has_extract = plan
            .sub_goals
            .iter()
            .filter(|sg| sg.action_type == ActionType::Extract)
            .count();
        assert!(
            has_extract >= 2,
            "Analys-plan borde ha minst 2 extraheringssteg för bred analys"
        );
    }

    #[test]
    fn test_compile_nyheter_goal() {
        let plan = compile_goal("hitta senaste nyheterna");
        let has_nyheter_step = plan
            .sub_goals
            .iter()
            .any(|sg| sg.description.to_lowercase().contains("nyhet"));
        assert!(
            has_nyheter_step,
            "Nyhets-mål borde generera nyhetsspecifika steg"
        );
    }

    #[test]
    fn test_execution_order_parallel() {
        let plan = compile_goal("registrera nytt konto");
        // Registrering har parallella Fill-steg (namn, email, lösenord)
        let has_parallel = plan.execution_order.iter().any(|group| group.len() > 1);

        // Det kan vara parallellt om deps tillåter det
        assert!(
            plan.parallel_groups > 0,
            "Borde ha minst 1 exekveringsgrupp"
        );
        // Registrering borde ha parallella Fill-steg
        assert!(has_parallel, "Registrering borde ha parallella steg");
    }

    #[test]
    fn test_execute_plan_next_action() {
        let plan = compile_goal("logga in");
        let tree = make_tree(vec![
            SemanticNode::new(1, "textbox", "E-postadress"),
            SemanticNode::new(2, "textbox", "Lösenord"),
            SemanticNode::new(3, "button", "Logga in"),
        ]);

        let result = execute_plan(&plan, &tree, &[]);
        assert!(result.next_action.is_some(), "Borde ha nästa action");
    }

    #[test]
    fn test_execute_plan_progress() {
        let plan = compile_goal("logga in");
        let tree = make_tree(vec![SemanticNode::new(1, "button", "Logga in")]);

        // Markera första steget som klart
        let result = execute_plan(&plan, &tree, &[0]);
        assert_eq!(result.current_step, 1);
        assert!(
            result.summary.contains("1/"),
            "Sammanfattning borde visa progress"
        );
    }

    #[test]
    fn test_compile_plan_serialization() {
        let plan = compile_goal("köp produkt");
        let json = serde_json::to_string(&plan).unwrap_or_default();
        assert!(!json.is_empty());
        let restored: ActionPlan = serde_json::from_str(&json).expect("Borde gå att deserialisera");
        assert_eq!(restored.goal, plan.goal);
        assert_eq!(restored.sub_goals.len(), plan.sub_goals.len());
    }

    #[test]
    fn test_prefetch_suggestions() {
        let plan = compile_goal("köp produkt");
        let tree = make_tree(vec![]);
        let result = execute_plan(&plan, &tree, &[]);

        // Borde ha Navigate-steg som föreslås för prefetch
        // (depends on plan structure, but usually the first navigate is ready)
        // Prefetch suggestions come from non-completed Navigate steps
        assert!(
            result
                .plan
                .sub_goals
                .iter()
                .any(|sg| sg.action_type == ActionType::Navigate),
            "Köp-plan borde ha Navigate-steg"
        );
    }
}
