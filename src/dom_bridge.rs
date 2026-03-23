/// QuickJS DOM Bridge — Fas 17.4-17.6 (stubbed for compilation)
///
/// Temporärt stubbad version medans fullständig JsHandler-baserad omskrivning pågår.
/// Exponerar ArenaDom som `document`/`window`-objekt i QuickJS-kontexten.
use crate::arena_dom::ArenaDom;

/// Resultat från DOM-medveten JS-evaluering
#[derive(Debug, Clone)]
pub struct DomEvalResult {
    /// Eventuellt returvärde som sträng
    pub value: Option<String>,
    /// Felmeddelande om evalueringen misslyckades
    pub error: Option<String>,
    /// Lista av DOM-mutationer som JS:en utförde
    pub mutations: Vec<DomMutation>,
    /// Exekveringstid i mikrosekunder
    pub eval_time_us: u64,
    /// Event-loop-statistik (ticks, timers, rAF)
    pub event_loop_ticks: usize,
    /// Antal timer-callbacks som kördes
    pub timers_fired: usize,
}

/// En mutation som JS-koden utförde på DOM:en
pub type DomMutation = String;

/// Evaluera JS-kod med tillgång till DOM via ArenaDom
///
/// Sätter upp `document` och `window` som globala objekt i QuickJS-kontexten.
/// Returnerar eventuellt returvärde och alla DOM-mutationer.
pub fn eval_js_with_dom(code: &str, arena: ArenaDom) -> DomEvalResult {
    let start = std::time::Instant::now();

    // Säkerhetskontroll: blockera farliga operationer
    let lower = code.to_lowercase();
    for forbidden in &[
        "fetch(",
        "xmlhttp",
        "import(",
        "require(",
        "eval(",
        "new worker",
        "indexeddb",
    ] {
        if lower.contains(forbidden) {
            return DomEvalResult {
                value: None,
                error: Some(format!(
                    "Blocked: '{}' is not allowed in sandbox",
                    forbidden.trim_end_matches('(')
                )),
                mutations: vec![],
                eval_time_us: start.elapsed().as_micros() as u64,
                event_loop_ticks: 0,
                timers_fired: 0,
            };
        }
    }

    #[cfg(feature = "js-eval")]
    {
        use crate::event_loop::{self, EventLoopState, JsFn, JsHandler, SharedEventLoop};
        use rquickjs::{Ctx, Function, Object, Value};
        use std::cell::RefCell;
        use std::rc::Rc;

        // Skapa sandboxad QuickJS-kontext
        let (rt, context) = crate::js_eval::create_sandboxed_runtime();

        // Enkel JS-evaluering utan full DOM-bridge (stubbad)
        let result = context.with(|ctx| match ctx.eval::<Value, _>(code) {
            Ok(result) => {
                let value_str = crate::js_eval::quickjs_value_to_string(&ctx, &result);
                DomEvalResult {
                    value: if value_str == "undefined" {
                        None
                    } else {
                        Some(value_str)
                    },
                    error: None,
                    mutations: vec![],
                    eval_time_us: start.elapsed().as_micros() as u64,
                    event_loop_ticks: 0,
                    timers_fired: 0,
                }
            }
            Err(e) => {
                let err_str = crate::js_eval::quickjs_error_string(&ctx, &e);
                DomEvalResult {
                    value: None,
                    error: Some(err_str),
                    mutations: vec![],
                    eval_time_us: start.elapsed().as_micros() as u64,
                    event_loop_ticks: 0,
                    timers_fired: 0,
                }
            }
        });
        // Arena droppas här — den stubbade versionen modifierar den inte
        let _ = arena;
        result
    }
    #[cfg(not(feature = "js-eval"))]
    {
        let _ = arena;
        DomEvalResult {
            value: None,
            error: Some("JS evaluation not available (js-eval feature disabled)".to_string()),
            mutations: vec![],
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: 0,
            timers_fired: 0,
        }
    }
}

/// Resultat med modifierad ArenaDom — för render_with_js-pipeline
#[cfg(feature = "blitz")]
pub struct DomEvalWithArena {
    pub result: DomEvalResult,
    pub arena: ArenaDom,
}

/// Evaluera JS med DOM-access och returnera den modifierade ArenaDom
///
/// Samma som `eval_js_with_dom` men ger tillbaka arena efter JS-evaluering
/// så att anroparen kan serialisera den modifierade DOM:en.
#[cfg(feature = "blitz")]
pub fn eval_js_with_dom_and_arena(code: &str, arena: ArenaDom) -> DomEvalWithArena {
    let result = eval_js_with_dom(code, arena.clone());
    DomEvalWithArena { result, arena }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blocked_operations() {
        let arena = crate::arena_dom::ArenaDom::new();
        let result = eval_js_with_dom("fetch('http://evil.com')", arena);
        assert!(result.error.is_some(), "fetch borde vara blockerat");
        assert!(
            result.error.as_ref().unwrap().contains("Blocked"),
            "Felmeddelande borde nämna 'Blocked'"
        );
    }

    #[test]
    fn test_eval_basic() {
        let arena = crate::arena_dom::ArenaDom::new();
        let result = eval_js_with_dom("1 + 2", arena);
        assert_eq!(result.value, Some("3".to_string()));
        assert!(result.error.is_none());
    }
}
