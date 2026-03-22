/// Event Loop — Fas 18
///
/// Händelseloop för Boa JS-sandboxen. Ger stöd för:
/// - Microtask-kö (Promise.then, queueMicrotask) via Boas inbyggda SimpleJobExecutor
/// - setTimeout / setInterval (begränsade: max 100 timers, max 5000ms delay)
/// - clearTimeout / clearInterval
/// - requestAnimationFrame (simulerad med 16ms tick)
/// - cancelAnimationFrame
/// - MutationObserver (kopplad till ArenaDom)
///
/// Alla timer-callbacks körs synkront via virtuell klocka — ingen riktig väntan.
/// Säkerhetsbegränsningar: max 1000 ticks, max 50ms total exekvering.
use boa_engine::{
    js_string, object::ObjectInitializer, property::Attribute, Context, JsArgs, JsValue,
    NativeFunction,
};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// Max antal timers som kan registreras (förhindrar oändlig timer-skapning)
const MAX_TIMERS: usize = 100;

/// Max delay för setTimeout/setInterval (ms)
const MAX_DELAY_MS: u64 = 5000;

/// Max antal ticks i event-loopen (förhindrar oändliga loopar)
const MAX_TICKS: usize = 1000;

/// Max total körtid för event-loopen (µs)
const MAX_RUNTIME_US: u64 = 50_000;

/// Simulerad rAF-intervall (ms) — ~60fps
const RAF_INTERVAL_MS: u64 = 16;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// En timer-uppgift (setTimeout/setInterval)
#[derive(Clone)]
struct TimerTask {
    /// Unikt ID
    id: u32,
    /// JS-callback som ska anropas
    callback: boa_engine::JsValue,
    /// Tid kvar till exekvering (ms)
    delay_ms: u64,
    /// Om detta är en interval-timer (ska upprepas)
    recurring: bool,
    /// Avbruten?
    cancelled: bool,
}

/// En rAF-callback
#[derive(Clone)]
struct RafTask {
    /// Unikt ID
    id: u32,
    /// JS-callback
    callback: boa_engine::JsValue,
    /// Avbruten?
    cancelled: bool,
}

/// En MutationObserver-registrering
#[derive(Clone)]
struct ObserverEntry {
    /// JS-callback
    callback: boa_engine::JsValue,
    /// Observerade noder (NodeKey som u64)
    targets: Vec<u64>,
    /// Konfiguration
    child_list: bool,
    attributes: bool,
    subtree: bool,
}

/// En DOM-mutation som observerats
#[derive(Debug, Clone)]
pub struct MutationRecord {
    /// Typ: "childList", "attributes", "characterData"
    pub mutation_type: String,
    /// Target-nod (som NodeKey u64)
    pub target: u64,
    /// Attributnamn (om type == "attributes")
    pub attribute_name: Option<String>,
}

/// Delad state för event-loopen
pub struct EventLoopState {
    /// Timer-kö
    timers: Vec<TimerTask>,
    /// rAF-kö
    raf_queue: Vec<RafTask>,
    /// MutationObserver-registreringar
    observers: Vec<ObserverEntry>,
    /// Kö av mutations som ska levereras till observers
    pending_mutations: VecDeque<MutationRecord>,
    /// Nästa timer/rAF-ID
    next_id: u32,
    /// Virtuell klocka (ms)
    virtual_time_ms: u64,
    /// Antal ticks som körts
    ticks: usize,
}

pub type SharedEventLoop = Rc<RefCell<EventLoopState>>;

impl EventLoopState {
    /// Skapa ny event-loop-state
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            raf_queue: Vec::new(),
            observers: Vec::new(),
            pending_mutations: VecDeque::new(),
            next_id: 1,
            virtual_time_ms: 0,
            ticks: 0,
        }
    }

    /// Allokera nästa unika ID
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Kolla om det finns väntande arbete
    fn has_pending_work(&self) -> bool {
        !self.timers.iter().all(|t| t.cancelled)
            || !self.raf_queue.iter().all(|r| r.cancelled)
            || !self.pending_mutations.is_empty()
    }
}

// ─── Registrera globala funktioner ──────────────────────────────────────────

/// Hjälpfunktion: registrera en NativeFunction som global
fn register_global_fn(context: &mut Context, name: &str, func: NativeFunction) {
    let js_func = func.to_js_function(context.realm());
    context
        .register_global_property(js_string!(name), JsValue::from(js_func), Attribute::all())
        .unwrap_or(());
}

/// Registrera alla event-loop-globaler på Boa-kontexten
pub fn register_event_loop(context: &mut Context, el: SharedEventLoop) {
    register_timers(context, Rc::clone(&el));
    register_raf(context, Rc::clone(&el));
    register_queue_microtask(context);
    register_mutation_observer(context, el);
}

/// Registrera setTimeout, setInterval, clearTimeout, clearInterval
fn register_timers(context: &mut Context, el: SharedEventLoop) {
    // setTimeout(callback, delay)
    let el_st = Rc::clone(&el);
    let set_timeout = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let callback = args.get_or_undefined(0).clone();
            if !callback.is_callable() {
                return Ok(JsValue::from(0));
            }
            let delay = args
                .get_or_undefined(1)
                .to_number(_ctx)
                .unwrap_or(0.0)
                .max(0.0) as u64;
            let delay = delay.min(MAX_DELAY_MS);

            let mut state = el_st.borrow_mut();
            if state.timers.len() >= MAX_TIMERS {
                return Ok(JsValue::from(-1));
            }
            let id = state.alloc_id();
            state.timers.push(TimerTask {
                id,
                callback,
                delay_ms: delay,
                recurring: false,
                cancelled: false,
            });
            Ok(JsValue::from(id))
        })
    };
    register_global_fn(context, "setTimeout", set_timeout);

    // setInterval(callback, delay)
    let el_si = Rc::clone(&el);
    let set_interval = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let callback = args.get_or_undefined(0).clone();
            if !callback.is_callable() {
                return Ok(JsValue::from(0));
            }
            let delay = args
                .get_or_undefined(1)
                .to_number(_ctx)
                .unwrap_or(0.0)
                .max(1.0) as u64;
            let delay = delay.min(MAX_DELAY_MS);

            let mut state = el_si.borrow_mut();
            if state.timers.len() >= MAX_TIMERS {
                return Ok(JsValue::from(-1));
            }
            let id = state.alloc_id();
            state.timers.push(TimerTask {
                id,
                callback,
                delay_ms: delay,
                recurring: true,
                cancelled: false,
            });
            Ok(JsValue::from(id))
        })
    };
    register_global_fn(context, "setInterval", set_interval);

    // clearTimeout(id)
    let el_ct = Rc::clone(&el);
    let clear_timeout = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let id = args.get_or_undefined(0).to_number(_ctx).unwrap_or(0.0) as u32;
            let mut state = el_ct.borrow_mut();
            for timer in &mut state.timers {
                if timer.id == id {
                    timer.cancelled = true;
                }
            }
            Ok(JsValue::undefined())
        })
    };
    register_global_fn(context, "clearTimeout", clear_timeout);

    // clearInterval(id) — samma logik
    let el_ci = Rc::clone(&el);
    let clear_interval = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let id = args.get_or_undefined(0).to_number(_ctx).unwrap_or(0.0) as u32;
            let mut state = el_ci.borrow_mut();
            for timer in &mut state.timers {
                if timer.id == id {
                    timer.cancelled = true;
                }
            }
            Ok(JsValue::undefined())
        })
    };
    register_global_fn(context, "clearInterval", clear_interval);
}

/// Registrera requestAnimationFrame / cancelAnimationFrame
fn register_raf(context: &mut Context, el: SharedEventLoop) {
    let el_raf = Rc::clone(&el);
    let raf = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let callback = args.get_or_undefined(0).clone();
            if !callback.is_callable() {
                return Ok(JsValue::from(0));
            }
            let mut state = el_raf.borrow_mut();
            let id = state.alloc_id();
            state.raf_queue.push(RafTask {
                id,
                callback,
                cancelled: false,
            });
            Ok(JsValue::from(id))
        })
    };
    register_global_fn(context, "requestAnimationFrame", raf);

    let el_craf = Rc::clone(&el);
    let cancel_raf = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let id = args.get_or_undefined(0).to_number(_ctx).unwrap_or(0.0) as u32;
            let mut state = el_craf.borrow_mut();
            for raf_task in &mut state.raf_queue {
                if raf_task.id == id {
                    raf_task.cancelled = true;
                }
            }
            Ok(JsValue::undefined())
        })
    };
    register_global_fn(context, "cancelAnimationFrame", cancel_raf);
}

/// Registrera queueMicrotask (delegerar till Boas job-kö)
fn register_queue_microtask(context: &mut Context) {
    let queue_microtask = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let callback = args.get_or_undefined(0).clone();
        if let Some(callable) = callback.as_callable() {
            let promise_job = boa_engine::job::PromiseJob::new(move |ctx: &mut Context| {
                callable.call(&JsValue::undefined(), &[], ctx)
            });
            ctx.enqueue_job(boa_engine::job::Job::PromiseJob(promise_job));
        }
        Ok(JsValue::undefined())
    });
    register_global_fn(context, "queueMicrotask", queue_microtask);
}

/// Registrera MutationObserver-konstruktorn
fn register_mutation_observer(context: &mut Context, el: SharedEventLoop) {
    let el_mo = Rc::clone(&el);
    let mo_constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0).clone();
            if !callback.is_callable() {
                return Ok(JsValue::undefined());
            }

            // Skapa observer-objekt med observe() och disconnect()
            let observer_index = {
                let mut state = el_mo.borrow_mut();
                let idx = state.observers.len();
                state.observers.push(ObserverEntry {
                    callback,
                    targets: Vec::new(),
                    child_list: false,
                    attributes: false,
                    subtree: false,
                });
                idx
            };

            let el_observe = Rc::clone(&el_mo);
            let observe_fn = NativeFunction::from_closure(move |_this, args, _ctx| {
                // observe(targetNode, options)
                let target = args.get_or_undefined(0);
                let options = args.get_or_undefined(1);

                // Extrahera __nodeKey__ från target
                let node_key = target
                    .as_object()
                    .and_then(|obj| {
                        obj.get(js_string!("__nodeKey__"), _ctx)
                            .ok()
                            .and_then(|v| v.to_number(_ctx).ok())
                    })
                    .unwrap_or(0.0) as u64;

                // Läs options
                let child_list = options
                    .as_object()
                    .and_then(|obj| {
                        obj.get(js_string!("childList"), _ctx)
                            .ok()
                            .map(|v| v.to_boolean())
                    })
                    .unwrap_or(false);
                let attributes = options
                    .as_object()
                    .and_then(|obj| {
                        obj.get(js_string!("attributes"), _ctx)
                            .ok()
                            .map(|v| v.to_boolean())
                    })
                    .unwrap_or(false);
                let subtree = options
                    .as_object()
                    .and_then(|obj| {
                        obj.get(js_string!("subtree"), _ctx)
                            .ok()
                            .map(|v| v.to_boolean())
                    })
                    .unwrap_or(false);

                let mut state = el_observe.borrow_mut();
                if let Some(entry) = state.observers.get_mut(observer_index) {
                    entry.targets.push(node_key);
                    entry.child_list = child_list;
                    entry.attributes = attributes;
                    entry.subtree = subtree;
                }

                Ok(JsValue::undefined())
            });

            let el_disconnect = Rc::clone(&el_mo);
            let disconnect_fn = NativeFunction::from_closure(move |_this, _args, _ctx| {
                let mut state = el_disconnect.borrow_mut();
                if let Some(entry) = state.observers.get_mut(observer_index) {
                    entry.targets.clear();
                }
                Ok(JsValue::undefined())
            });

            let observer = ObjectInitializer::new(ctx)
                .function(observe_fn, js_string!("observe"), 2)
                .function(disconnect_fn, js_string!("disconnect"), 0)
                .build();

            Ok(observer.into())
        })
    };

    register_global_fn(context, "MutationObserver", mo_constructor);
}

// ─── Event Loop Runner ──────────────────────────────────────────────────────

/// Kör event-loopen tills alla köer är tomma eller begränsningar nås
///
/// Returnerar antal ticks som kördes och eventuella fel.
pub fn run_event_loop(
    context: &mut Context,
    el: &SharedEventLoop,
) -> Result<EventLoopStats, String> {
    let mut total_ticks: usize = 0;
    let mut timers_fired: usize = 0;
    let mut rafs_fired: usize = 0;
    let mut mutations_delivered: usize = 0;

    // Tidsbegränsning för hela event-loopen
    let wall_start = std::time::Instant::now();

    // Fas 1: Dränera Boas inbyggda microtask-kö (Promises)
    if let Err(e) = context.run_jobs() {
        return Err(format!("Microtask error: {}", e));
    }
    total_ticks += 1;

    loop {
        if total_ticks >= MAX_TICKS {
            break;
        }
        if wall_start.elapsed().as_micros() as u64 > MAX_RUNTIME_US {
            break;
        }

        let has_work = el.borrow().has_pending_work();
        if !has_work {
            break;
        }

        // Fas 2: Avancera virtuell klocka och kör mogna timers
        let due_timers = {
            let mut state = el.borrow_mut();
            state.virtual_time_ms += 1; // Avancera 1ms per tick
            state.ticks += 1;

            let current_time = state.virtual_time_ms;
            let mut due = Vec::new();
            let mut recurring_to_readd = Vec::new();

            // Ta ut timers som är mogna
            let timers = std::mem::take(&mut state.timers);
            for timer in timers {
                if timer.cancelled {
                    continue;
                }
                if timer.delay_ms <= current_time {
                    due.push(timer.clone());
                    if timer.recurring {
                        // Schemalägg om med nytt delay
                        recurring_to_readd.push(TimerTask {
                            delay_ms: current_time + timer.delay_ms.max(1),
                            ..timer
                        });
                    }
                } else {
                    // Inte mogen ännu — behåll
                    state.timers.push(timer);
                }
            }
            state.timers.extend(recurring_to_readd);
            due
        };

        for timer in &due_timers {
            if let Some(callable) = timer.callback.as_callable() {
                let _ = callable.call(&JsValue::undefined(), &[], context);
            }
            timers_fired += 1;
        }

        // Fas 3: Kör rAF-callbacks (en gång per ~16ms virtuell tid)
        let should_fire_raf = {
            let state = el.borrow();
            state.virtual_time_ms.is_multiple_of(RAF_INTERVAL_MS) && !state.raf_queue.is_empty()
        };

        if should_fire_raf {
            let raf_tasks = {
                let mut state = el.borrow_mut();
                std::mem::take(&mut state.raf_queue)
            };

            let timestamp = el.borrow().virtual_time_ms as f64;
            for raf in &raf_tasks {
                if raf.cancelled {
                    continue;
                }
                if let Some(callable) = raf.callback.as_callable() {
                    let _ =
                        callable.call(&JsValue::undefined(), &[JsValue::from(timestamp)], context);
                }
                rafs_fired += 1;
            }
        }

        // Fas 4: Leverera MutationObserver-mutations
        let pending = {
            let mut state = el.borrow_mut();
            std::mem::take(&mut state.pending_mutations)
                .into_iter()
                .collect::<Vec<_>>()
        };

        if !pending.is_empty() {
            let observers = el.borrow().observers.clone();
            for observer in &observers {
                if observer.targets.is_empty() {
                    continue;
                }

                // Filtrera mutations som matchar observerade targets
                let matching: Vec<&MutationRecord> = pending
                    .iter()
                    .filter(|m| {
                        observer.targets.contains(&m.target)
                            && match m.mutation_type.as_str() {
                                "childList" => observer.child_list,
                                "attributes" => observer.attributes,
                                _ => false,
                            }
                    })
                    .collect();

                if matching.is_empty() {
                    continue;
                }

                // Skapa JS mutation records array
                if let Some(callable) = observer.callback.as_callable() {
                    let records = boa_engine::object::builtins::JsArray::new(context);
                    for (i, mr) in matching.iter().enumerate() {
                        let record_obj = ObjectInitializer::new(context)
                            .property(
                                js_string!("type"),
                                JsValue::from(js_string!(mr.mutation_type.as_str())),
                                Attribute::all(),
                            )
                            .property(
                                js_string!("target"),
                                JsValue::from(mr.target as f64),
                                Attribute::all(),
                            )
                            .build();

                        if let Some(attr) = &mr.attribute_name {
                            let _ = record_obj.set(
                                js_string!("attributeName"),
                                JsValue::from(js_string!(attr.as_str())),
                                false,
                                context,
                            );
                        }

                        let _ = records.set(i as u32, record_obj, false, context);
                    }
                    let _ = callable.call(&JsValue::undefined(), &[records.into()], context);
                    mutations_delivered += matching.len();
                }
            }
        }

        // Fas 5: Dränera microtasks igen (timer/rAF callbacks kan ha schemalagt nya)
        if let Err(e) = context.run_jobs() {
            return Err(format!("Microtask error in tick {}: {}", total_ticks, e));
        }

        total_ticks += 1;

        // Kolla om allt arbete är klart
        let still_work = el.borrow().has_pending_work();
        // Kolla också Boas interna kö
        if !still_work {
            break;
        }
    }

    Ok(EventLoopStats {
        ticks: total_ticks,
        timers_fired,
        rafs_fired,
        mutations_delivered,
    })
}

/// Statistik från event-loop-körningen
#[derive(Debug, Clone)]
pub struct EventLoopStats {
    /// Antal ticks som kördes
    pub ticks: usize,
    /// Antal timer-callbacks som kördes
    pub timers_fired: usize,
    /// Antal rAF-callbacks som kördes
    pub rafs_fired: usize,
    /// Antal mutation-records som levererades
    pub mutations_delivered: usize,
}

impl std::fmt::Display for EventLoopStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ticks={}, timers={}, rafs={}, mutations={}",
            self.ticks, self.timers_fired, self.rafs_fired, self.mutations_delivered
        )
    }
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_state_creation() {
        let state = EventLoopState::new();
        assert_eq!(state.next_id, 1, "Första ID borde vara 1");
        assert_eq!(state.virtual_time_ms, 0, "Virtuell tid borde starta på 0");
        assert!(state.timers.is_empty(), "Inga timers vid start");
        assert!(state.raf_queue.is_empty(), "Ingen rAF-kö vid start");
        assert!(state.observers.is_empty(), "Inga observers vid start");
    }

    #[test]
    fn test_alloc_id_increments() {
        let mut state = EventLoopState::new();
        assert_eq!(state.alloc_id(), 1);
        assert_eq!(state.alloc_id(), 2);
        assert_eq!(state.alloc_id(), 3);
    }

    #[test]
    fn test_has_pending_work_empty() {
        let state = EventLoopState::new();
        assert!(!state.has_pending_work(), "Tom state ska inte ha arbete");
    }

    #[test]
    fn test_has_pending_work_with_mutation() {
        let mut state = EventLoopState::new();
        state.pending_mutations.push_back(MutationRecord {
            mutation_type: "childList".to_string(),
            target: 1,
            attribute_name: None,
        });
        assert!(
            state.has_pending_work(),
            "State med mutation borde ha arbete"
        );
    }

    #[test]
    fn test_promise_resolution() {
        // Testa att Promises resolveras via run_jobs
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        // Skapa en Promise som sätter en global variabel
        let code = r#"
            var resolved = false;
            Promise.resolve(42).then(function(v) {
                resolved = true;
            });
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        // Kör event-loopen
        let stats = run_event_loop(&mut context, &el);
        assert!(stats.is_ok(), "Event loop borde lyckas");

        // Kolla att resolved = true
        let check = context.eval(boa_engine::Source::from_bytes("resolved"));
        assert!(check.is_ok(), "Borde kunna läsa resolved");
        let val = check.unwrap();
        assert_eq!(
            val.to_boolean(),
            true,
            "resolved borde vara true efter Promise.then"
        );
    }

    #[test]
    fn test_set_timeout_basic() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var timerFired = false;
            var timerId = setTimeout(function() {
                timerFired = true;
            }, 1);
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let stats = run_event_loop(&mut context, &el);
        assert!(stats.is_ok(), "Event loop borde lyckas");
        let stats = stats.unwrap();
        assert!(stats.timers_fired > 0, "Minst en timer borde ha körts");

        let check = context.eval(boa_engine::Source::from_bytes("timerFired"));
        assert_eq!(
            check.unwrap().to_boolean(),
            true,
            "timerFired borde vara true"
        );
    }

    #[test]
    fn test_clear_timeout() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var timerFired = false;
            var id = setTimeout(function() {
                timerFired = true;
            }, 1);
            clearTimeout(id);
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("timerFired"));
        assert_eq!(
            check.unwrap().to_boolean(),
            false,
            "timerFired borde vara false efter clearTimeout"
        );
    }

    #[test]
    fn test_set_interval_fires_multiple() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var count = 0;
            var id = setInterval(function() {
                count++;
                if (count >= 3) {
                    clearInterval(id);
                }
            }, 1);
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("count"));
        let val = check.unwrap().to_number(&mut context).unwrap_or(0.0) as i32;
        assert!(
            val >= 3,
            "count borde vara >= 3 efter setInterval: got {}",
            val
        );
    }

    #[test]
    fn test_request_animation_frame() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var rafCalled = false;
            var rafTimestamp = 0;
            requestAnimationFrame(function(ts) {
                rafCalled = true;
                rafTimestamp = ts;
            });
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let stats = run_event_loop(&mut context, &el);
        assert!(stats.is_ok(), "Event loop borde lyckas");

        let check = context.eval(boa_engine::Source::from_bytes("rafCalled"));
        assert_eq!(
            check.unwrap().to_boolean(),
            true,
            "rAF callback borde ha körts"
        );
    }

    #[test]
    fn test_cancel_animation_frame() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var rafCalled = false;
            var id = requestAnimationFrame(function(ts) {
                rafCalled = true;
            });
            cancelAnimationFrame(id);
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("rafCalled"));
        assert_eq!(
            check.unwrap().to_boolean(),
            false,
            "rAF borde inte ha körts efter cancel"
        );
    }

    #[test]
    fn test_queue_microtask() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var microtaskRan = false;
            queueMicrotask(function() {
                microtaskRan = true;
            });
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("microtaskRan"));
        assert_eq!(
            check.unwrap().to_boolean(),
            true,
            "Microtask borde ha körts"
        );
    }

    #[test]
    fn test_max_timers_limit() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        // Registrera MAX_TIMERS + 1 timers
        let code = format!(
            r#"
            var results = [];
            for (var i = 0; i < {}; i++) {{
                results.push(setTimeout(function(){{}}, 100));
            }}
            var lastId = setTimeout(function(){{}}, 100);
        "#,
            MAX_TIMERS
        );
        let _ = context.eval(boa_engine::Source::from_bytes(code.as_bytes()));

        // lastId borde vara -1 (limit nådd)
        let check = context.eval(boa_engine::Source::from_bytes("lastId"));
        let val = check.unwrap().to_number(&mut context).unwrap_or(0.0) as i32;
        assert_eq!(val, -1, "Borde returnera -1 vid timer-limit");
    }

    #[test]
    fn test_event_loop_stats() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            setTimeout(function() {}, 1);
            requestAnimationFrame(function(ts) {});
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));

        let stats = run_event_loop(&mut context, &el).unwrap();
        assert!(stats.ticks > 0, "Borde ha kört minst 1 tick");
        // Kolla att Display-impl fungerar (använder alla fält)
        let display = format!("{}", stats);
        assert!(
            display.contains("ticks="),
            "Display borde innehålla ticks: {}",
            display
        );
        assert!(
            display.contains("rafs="),
            "Display borde innehålla rafs: {}",
            display
        );
        assert!(
            display.contains("mutations="),
            "Display borde innehålla mutations: {}",
            display
        );
    }

    #[test]
    fn test_mutation_record() {
        let mut state = EventLoopState::new();
        state.pending_mutations.push_back(MutationRecord {
            mutation_type: "attributes".to_string(),
            target: 42,
            attribute_name: Some("class".to_string()),
        });
        assert_eq!(
            state.pending_mutations.len(),
            1,
            "Borde ha 1 pending mutation"
        );
        let mr = state.pending_mutations.pop_front().unwrap();
        assert_eq!(mr.mutation_type, "attributes");
        assert_eq!(mr.target, 42);
        assert_eq!(mr.attribute_name, Some("class".to_string()));
    }

    #[test]
    fn test_promise_chain() {
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var result = 0;
            Promise.resolve(1)
                .then(function(v) { return v + 1; })
                .then(function(v) { return v * 10; })
                .then(function(v) { result = v; });
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));
        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("result"));
        let val = check.unwrap().to_number(&mut context).unwrap_or(0.0) as i32;
        assert_eq!(val, 20, "Promise-kedja borde ge 20: got {}", val);
    }

    #[test]
    fn test_setTimeout_with_promise() {
        // Kombinera timers och promises
        let mut context = Context::default();
        let el = Rc::new(RefCell::new(EventLoopState::new()));
        register_event_loop(&mut context, Rc::clone(&el));

        let code = r#"
            var steps = [];
            steps.push("start");
            setTimeout(function() {
                steps.push("timeout");
            }, 1);
            Promise.resolve().then(function() {
                steps.push("promise");
            });
        "#;
        let _ = context.eval(boa_engine::Source::from_bytes(code));
        let _ = run_event_loop(&mut context, &el);

        let check = context.eval(boa_engine::Source::from_bytes("steps.join(',')"));
        let val = check
            .unwrap()
            .to_string(&mut context)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        assert!(
            val.contains("start"),
            "Borde innehålla 'start': got {}",
            val
        );
        assert!(
            val.contains("promise"),
            "Borde innehålla 'promise': got {}",
            val
        );
        assert!(
            val.contains("timeout"),
            "Borde innehålla 'timeout': got {}",
            val
        );
    }
}
