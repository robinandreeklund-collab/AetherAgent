/// Event Loop — Fas 18
///
/// Händelseloop för QuickJS-sandboxen. Ger stöd för:
/// - Microtask-kö (Promise.then, queueMicrotask) via QuickJS inbyggda job-kö
/// - setTimeout / setInterval (begränsade: max 100 timers, max 5000ms delay)
/// - clearTimeout / clearInterval
/// - requestAnimationFrame (simulerad med 16ms tick)
/// - cancelAnimationFrame
/// - MutationObserver (kopplad till ArenaDom)
///
/// Alla timer-callbacks körs synkront via virtuell klocka — ingen riktig väntan.
/// Säkerhetsbegränsningar: max 1000 ticks, max 50ms total exekvering.
use rquickjs::{
    function::{FromParams, IntoJsFunc, ParamRequirement, Params, Rest},
    Ctx, Function, Object, Persistent, Value,
};

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// Typ-alias för due timers och recurring timers att spara om.
type DueTimersResult = (
    Vec<Persistent<Function<'static>>>,
    Vec<(u32, Persistent<Function<'static>>, u64)>,
);

// ─── Livstidssäker IntoJsFunc-wrapper ────────────────────────────────────────
// rquickjs closures kan inte returnera Value<'js> pga livstidsinferens.
// Denna trait + wrapper löser problemet via explicit 'js-livstid i handle().

/// Trait för JS-callback-hanterare med korrekt livstidshantering
pub(crate) trait JsHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>>;
}

/// Wrapper som implementerar IntoJsFunc för alla JsHandler-implementationer
pub(crate) struct JsFn<H: JsHandler>(pub(crate) H);

impl<'js, H: JsHandler> IntoJsFunc<'js, (Ctx<'js>, Rest<Value<'js>>)> for JsFn<H> {
    fn param_requirements() -> ParamRequirement {
        ParamRequirement::any()
    }
    fn call(&self, params: Params<'_, 'js>) -> rquickjs::Result<Value<'js>> {
        let ctx = params.ctx().clone();
        let mut access = params.access();
        let (_, rest): (Ctx<'js>, Rest<Value<'js>>) = FromParams::from_params(&mut access)?;
        self.0.handle(&ctx, &rest.0)
    }
}

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// Max antal timers som kan registreras (förhindrar oändlig timer-skapning)
const MAX_TIMERS: usize = 500;

/// Max delay för setTimeout/setInterval (ms)
const MAX_DELAY_MS: u64 = 5000;

/// Max antal ticks i event-loopen (förhindrar oändliga loopar)
const MAX_TICKS: usize = 5000;

/// Max total körtid för event-loopen (µs) — 500ms ger utrymme för WPT-tester
const MAX_RUNTIME_US: u64 = 500_000;

/// Simulerad rAF-intervall (ms) — ~60fps
const RAF_INTERVAL_MS: u64 = 16;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// En timer-uppgift (setTimeout/setInterval)
struct TimerTask {
    /// Unikt ID
    id: u32,
    /// JS-callback som ska anropas (persistent — överlever GC)
    callback: Persistent<Function<'static>>,
    /// Tid kvar till exekvering (ms)
    delay_ms: u64,
    /// Om detta är en interval-timer (ska upprepas)
    recurring: bool,
    /// Avbruten?
    cancelled: bool,
}

/// En rAF-callback
struct RafTask {
    /// Unikt ID
    id: u32,
    /// JS-callback
    callback: Persistent<Function<'static>>,
    /// Avbruten?
    cancelled: bool,
}

/// En MutationObserver-registrering
struct ObserverEntry {
    /// JS-callback
    callback: Persistent<Function<'static>>,
    /// Observerade noder (NodeKey som u64)
    targets: Vec<u64>,
    /// Konfiguration
    child_list: bool,
    attributes: bool,
    #[allow(dead_code)]
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

    /// Rensa alla Persistent-referenser (måste anropas innan Runtime droppas)
    pub fn clear_persistent(&mut self) {
        self.timers.clear();
        self.raf_queue.clear();
        self.observers.clear();
    }

    /// Kolla om det finns väntande arbete
    fn has_pending_work(&self) -> bool {
        !self.timers.iter().all(|t| t.cancelled)
            || !self.raf_queue.iter().all(|r| r.cancelled)
            || !self.pending_mutations.is_empty()
    }
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Hämta argument som f64, med fallback
fn arg_as_f64(args: &[Value], index: usize) -> f64 {
    args.get(index)
        .and_then(|v| v.as_float().or_else(|| v.as_int().map(|i| i as f64)))
        .unwrap_or(0.0)
}

// ─── Registrera globala funktioner ──────────────────────────────────────────

/// Registrera alla event-loop-globaler på QuickJS-kontexten
pub fn register_event_loop<'js>(ctx: &Ctx<'js>, el: SharedEventLoop) -> rquickjs::Result<()> {
    register_timers(ctx, Rc::clone(&el))?;
    register_raf(ctx, Rc::clone(&el))?;
    register_queue_microtask(ctx)?;
    register_mutation_observer(ctx, el)?;
    Ok(())
}

/// Registrera setTimeout, setInterval, clearTimeout, clearInterval
fn register_timers(ctx: &Ctx<'_>, el: SharedEventLoop) -> rquickjs::Result<()> {
    // ─── Handler-strukturer ───────────────────────────────────────────
    struct SetTimerHandler {
        el: SharedEventLoop,
        recurring: bool,
    }
    impl JsHandler for SetTimerHandler {
        fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
            let func = match args.first().and_then(|v| v.as_function()) {
                Some(f) => f.clone(),
                None => return Ok(Value::new_int(ctx.clone(), 0)),
            };
            let delay = arg_as_f64(args, 1).max(if self.recurring { 1.0 } else { 0.0 }) as u64;
            let delay = delay.min(MAX_DELAY_MS);

            let mut state = self.el.borrow_mut();
            if state.timers.len() >= MAX_TIMERS {
                return Ok(Value::new_int(ctx.clone(), -1));
            }
            let id = state.alloc_id();
            state.timers.push(TimerTask {
                id,
                callback: Persistent::save(ctx, func),
                delay_ms: delay,
                recurring: self.recurring,
                cancelled: false,
            });
            Ok(Value::new_int(ctx.clone(), id as i32))
        }
    }

    struct ClearTimerHandler {
        el: SharedEventLoop,
    }
    impl JsHandler for ClearTimerHandler {
        fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
            let id = arg_as_f64(args, 0) as u32;
            let mut state = self.el.borrow_mut();
            for timer in &mut state.timers {
                if timer.id == id {
                    timer.cancelled = true;
                }
            }
            Ok(Value::new_undefined(ctx.clone()))
        }
    }

    ctx.globals().set(
        "setTimeout",
        Function::new(
            ctx.clone(),
            JsFn(SetTimerHandler {
                el: Rc::clone(&el),
                recurring: false,
            }),
        )?,
    )?;
    ctx.globals().set(
        "setInterval",
        Function::new(
            ctx.clone(),
            JsFn(SetTimerHandler {
                el: Rc::clone(&el),
                recurring: true,
            }),
        )?,
    )?;
    ctx.globals().set(
        "clearTimeout",
        Function::new(ctx.clone(), JsFn(ClearTimerHandler { el: Rc::clone(&el) }))?,
    )?;
    ctx.globals().set(
        "clearInterval",
        Function::new(ctx.clone(), JsFn(ClearTimerHandler { el: Rc::clone(&el) }))?,
    )?;

    Ok(())
}

/// Registrera requestAnimationFrame / cancelAnimationFrame
fn register_raf(ctx: &Ctx<'_>, el: SharedEventLoop) -> rquickjs::Result<()> {
    struct RafHandler {
        el: SharedEventLoop,
    }
    impl JsHandler for RafHandler {
        fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
            let func = match args.first().and_then(|v| v.as_function()) {
                Some(f) => f.clone(),
                None => return Ok(Value::new_int(ctx.clone(), 0)),
            };
            let mut state = self.el.borrow_mut();
            let id = state.alloc_id();
            state.raf_queue.push(RafTask {
                id,
                callback: Persistent::save(ctx, func),
                cancelled: false,
            });
            Ok(Value::new_int(ctx.clone(), id as i32))
        }
    }

    struct CancelRafHandler {
        el: SharedEventLoop,
    }
    impl JsHandler for CancelRafHandler {
        fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
            let id = arg_as_f64(args, 0) as u32;
            let mut state = self.el.borrow_mut();
            for raf_task in &mut state.raf_queue {
                if raf_task.id == id {
                    raf_task.cancelled = true;
                }
            }
            Ok(Value::new_undefined(ctx.clone()))
        }
    }

    ctx.globals().set(
        "requestAnimationFrame",
        Function::new(ctx.clone(), JsFn(RafHandler { el: Rc::clone(&el) }))?,
    )?;
    ctx.globals().set(
        "cancelAnimationFrame",
        Function::new(ctx.clone(), JsFn(CancelRafHandler { el: Rc::clone(&el) }))?,
    )?;

    Ok(())
}

/// Registrera queueMicrotask
///
/// QuickJS hanterar Promises internt — queueMicrotask schemaläggs
/// som en Promise.resolve().then(callback) under huven.
fn register_queue_microtask(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    // Implementera queueMicrotask som Promise.resolve().then(callback)
    ctx.eval::<(), _>(
        r#"
        globalThis.queueMicrotask = function(callback) {
            Promise.resolve().then(callback);
        };
        "#,
    )?;
    Ok(())
}

/// Registrera MutationObserver-konstruktorn
fn register_mutation_observer(ctx: &Ctx<'_>, el: SharedEventLoop) -> rquickjs::Result<()> {
    struct MutationObserverConstructor {
        el: SharedEventLoop,
    }
    impl JsHandler for MutationObserverConstructor {
        fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
            let func = match args.first().and_then(|v| v.as_function()) {
                Some(f) => f.clone(),
                None => return Ok(Value::new_undefined(ctx.clone())),
            };

            let observer_index = {
                let mut state = self.el.borrow_mut();
                let idx = state.observers.len();
                state.observers.push(ObserverEntry {
                    callback: Persistent::save(ctx, func),
                    targets: Vec::new(),
                    child_list: false,
                    attributes: false,
                    subtree: false,
                });
                idx
            };

            let observer = Object::new(ctx.clone())?;

            // observe(target, options)
            struct ObserveHandler {
                el: SharedEventLoop,
                idx: usize,
            }
            impl JsHandler for ObserveHandler {
                fn handle<'js>(
                    &self,
                    ctx: &Ctx<'js>,
                    args: &[Value<'js>],
                ) -> rquickjs::Result<Value<'js>> {
                    let target = args
                        .first()
                        .cloned()
                        .unwrap_or(Value::new_undefined(ctx.clone()));
                    let options = args
                        .get(1)
                        .cloned()
                        .unwrap_or(Value::new_undefined(ctx.clone()));

                    let node_key = target
                        .as_object()
                        .and_then(|obj| obj.get::<_, f64>("__nodeKey__").ok())
                        .unwrap_or(0.0) as u64;

                    let child_list = options
                        .as_object()
                        .and_then(|obj| obj.get::<_, bool>("childList").ok())
                        .unwrap_or(false);
                    let attributes = options
                        .as_object()
                        .and_then(|obj| obj.get::<_, bool>("attributes").ok())
                        .unwrap_or(false);
                    let subtree = options
                        .as_object()
                        .and_then(|obj| obj.get::<_, bool>("subtree").ok())
                        .unwrap_or(false);

                    let mut state = self.el.borrow_mut();
                    if let Some(entry) = state.observers.get_mut(self.idx) {
                        entry.targets.push(node_key);
                        entry.child_list = child_list;
                        entry.attributes = attributes;
                        entry.subtree = subtree;
                    }
                    Ok(Value::new_undefined(ctx.clone()))
                }
            }

            observer.set(
                "observe",
                Function::new(
                    ctx.clone(),
                    JsFn(ObserveHandler {
                        el: Rc::clone(&self.el),
                        idx: observer_index,
                    }),
                )?,
            )?;

            // disconnect()
            struct DisconnectHandler {
                el: SharedEventLoop,
                idx: usize,
            }
            impl JsHandler for DisconnectHandler {
                fn handle<'js>(
                    &self,
                    ctx: &Ctx<'js>,
                    _args: &[Value<'js>],
                ) -> rquickjs::Result<Value<'js>> {
                    let mut state = self.el.borrow_mut();
                    if let Some(entry) = state.observers.get_mut(self.idx) {
                        entry.targets.clear();
                    }
                    Ok(Value::new_undefined(ctx.clone()))
                }
            }

            observer.set(
                "disconnect",
                Function::new(
                    ctx.clone(),
                    JsFn(DisconnectHandler {
                        el: Rc::clone(&self.el),
                        idx: observer_index,
                    }),
                )?,
            )?;

            Ok(observer.into_value())
        }
    }

    // Registrera som __MutationObserverImpl och wrappa i en JS-klass för new-stöd
    ctx.globals().set(
        "__MutationObserverImpl",
        Function::new(
            ctx.clone(),
            JsFn(MutationObserverConstructor { el: Rc::clone(&el) }),
        )?,
    )?;
    ctx.eval::<Value, _>(
        r#"globalThis.MutationObserver = function MutationObserver(cb) {
            if (!(this instanceof MutationObserver)) throw new TypeError("not a constructor");
            var impl = __MutationObserverImpl(cb);
            this.observe = impl.observe;
            this.disconnect = impl.disconnect;
            this.takeRecords = impl.takeRecords || function(){return []};
        };"#,
    )?;

    Ok(())
}

// ─── Event Loop Runner ──────────────────────────────────────────────────────

/// Dränera QuickJS job-kö (Promises/microtasks)
/// Använder raw FFI för att undvika RefCell-dubbelborrow med Context::with
fn drain_pending_jobs_ctx(ctx: &Ctx<'_>) {
    unsafe {
        let ctx_ptr = ctx.as_raw().as_ptr();
        let rt_ptr = rquickjs::qjs::JS_GetRuntime(ctx_ptr);
        loop {
            if !rquickjs::qjs::JS_IsJobPending(rt_ptr) {
                break;
            }
            let mut pctx = std::mem::MaybeUninit::<*mut rquickjs::qjs::JSContext>::uninit();
            let r = rquickjs::qjs::JS_ExecutePendingJob(rt_ptr, pctx.as_mut_ptr());
            if r <= 0 {
                break;
            }
        }
    }
}

/// Kör event-loopen tills alla köer är tomma eller begränsningar nås
///
/// Returnerar antal ticks som kördes och eventuella fel.
pub fn run_event_loop(ctx: &Ctx<'_>, el: &SharedEventLoop) -> Result<EventLoopStats, String> {
    let mut total_ticks: usize = 0;
    let mut timers_fired: usize = 0;
    let mut rafs_fired: usize = 0;
    let mut mutations_delivered: usize = 0;

    // Tidsbegränsning för hela event-loopen
    let wall_start = std::time::Instant::now();

    // Fas 1: Dränera QuickJS inbyggda microtask-kö (Promises)
    drain_pending_jobs_ctx(ctx);
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
        // Steg 1: Extrahera mogna timers (utan att anropa restore inne i borrow_mut)
        let (due_timers, recurring_resave): DueTimersResult = {
            let mut state = el.borrow_mut();
            state.virtual_time_ms += 1; // Avancera 1ms per tick
            state.ticks += 1;

            let current_time = state.virtual_time_ms;
            let mut due = Vec::new();
            let mut recurring = Vec::new();
            let mut remaining = Vec::new();

            let timers = std::mem::take(&mut state.timers);
            for timer in timers {
                if timer.cancelled {
                    continue;
                }
                if timer.delay_ms <= current_time {
                    if timer.recurring {
                        recurring.push((
                            timer.id,
                            timer.callback,
                            current_time + timer.delay_ms.max(1),
                        ));
                    } else {
                        due.push(timer.callback);
                    }
                } else {
                    remaining.push(timer);
                }
            }
            state.timers = remaining;
            (due, recurring)
        };

        // Kör mogna one-shot callbacks
        for cb in due_timers {
            if let Ok(func) = cb.restore(ctx) {
                let _ = func.call::<_, Value>(());
            }
            timers_fired += 1;
        }

        // Kör och schemalägg om recurring timers
        for (id, cb, new_delay) in recurring_resave {
            if let Ok(func) = cb.restore(ctx) {
                let _ = func.call::<_, Value>(());
                // Schemalägg om
                let new_persistent = Persistent::save(ctx, func);
                let mut state = el.borrow_mut();
                state.timers.push(TimerTask {
                    id,
                    callback: new_persistent,
                    delay_ms: new_delay,
                    recurring: true,
                    cancelled: false,
                });
            }
            timers_fired += 1;
        }

        // Fas 3: Kör rAF-callbacks (en gång per ~16ms virtuell tid)
        let should_fire_raf = {
            let state = el.borrow();
            state.virtual_time_ms.is_multiple_of(RAF_INTERVAL_MS) && !state.raf_queue.is_empty()
        };

        if should_fire_raf {
            let raf_callbacks: Vec<(Persistent<Function<'static>>, bool)> = {
                let mut state = el.borrow_mut();
                let tasks = std::mem::take(&mut state.raf_queue);
                tasks
                    .into_iter()
                    .map(|t| (t.callback, t.cancelled))
                    .collect()
            };

            let timestamp = el.borrow().virtual_time_ms as f64;
            for (cb, cancelled) in raf_callbacks {
                if cancelled {
                    continue;
                }
                if let Ok(func) = cb.restore(ctx) {
                    let _ = func.call::<_, Value>((timestamp,));
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
            // Klona observer-info för att undvika dubbel-borrow
            let observer_info: Vec<(usize, Vec<u64>, bool, bool)> = {
                let state = el.borrow();
                state
                    .observers
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (i, o.targets.clone(), o.child_list, o.attributes))
                    .collect()
            };

            for (obs_idx, targets, child_list, attributes) in &observer_info {
                if targets.is_empty() {
                    continue;
                }

                let matching: Vec<&MutationRecord> = pending
                    .iter()
                    .filter(|m| {
                        targets.contains(&m.target)
                            && match m.mutation_type.as_str() {
                                "childList" => *child_list,
                                "attributes" => *attributes,
                                _ => false,
                            }
                    })
                    .collect();

                if matching.is_empty() {
                    continue;
                }

                // Hämta callback från observer via index
                let callback_persistent = {
                    let state = el.borrow();
                    state.observers.get(*obs_idx).map(|o| {
                        // Vi behöver klona Persistent — restore och re-save
                        o.callback.clone()
                    })
                };
                let Some(cb) = callback_persistent else {
                    continue;
                };
                if let Ok(func) = cb.restore(ctx) {
                    let records = rquickjs::Array::new(ctx.clone())
                        .map_err(|e| format!("Array::new failed: {}", e))
                        .unwrap_or_else(|_| rquickjs::Array::new(ctx.clone()).unwrap());

                    for (i, mr) in matching.iter().enumerate() {
                        if let Ok(record_obj) = Object::new(ctx.clone()) {
                            let _ = record_obj.set("type", mr.mutation_type.as_str());
                            let _ = record_obj.set("target", mr.target as f64);
                            if let Some(attr) = &mr.attribute_name {
                                let _ = record_obj.set("attributeName", attr.as_str());
                            }
                            let _ = records.set(i, record_obj);
                        }
                    }
                    let _ = func.call::<_, Value>((records,));
                    mutations_delivered += matching.len();
                }
            }
        }

        // Fas 5: Dränera microtasks igen (timer/rAF callbacks kan ha schemalagt nya)
        drain_pending_jobs_ctx(ctx);

        total_ticks += 1;

        // Kolla om allt arbete är klart
        let still_work = el.borrow().has_pending_work();
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

    #[cfg(feature = "js-eval")]
    fn with_quickjs_context<F, R>(f: F) -> R
    where
        F: for<'js> FnOnce(&rquickjs::Runtime, Ctx<'js>, SharedEventLoop) -> R,
    {
        let (rt, qctx, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();
        let result = qctx.with(|ctx| {
            let el = Rc::new(RefCell::new(EventLoopState::new()));
            register_event_loop(&ctx, Rc::clone(&el)).expect("register_event_loop borde lyckas");
            let result = f(&rt, ctx, Rc::clone(&el));
            el.borrow_mut().clear_persistent();
            result
        });
        crate::js_eval::free_interrupt_state(interrupt_ptr);
        result
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_promise_resolution() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var resolved = false;
                Promise.resolve(42).then(function(v) {
                    resolved = true;
                });
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let stats = run_event_loop(&ctx, &el);
            assert!(stats.is_ok(), "Event loop borde lyckas");

            let val: bool = ctx.eval("resolved").expect("borde kunna läsa resolved");
            assert!(val, "resolved borde vara true efter Promise.then");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_set_timeout_basic() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var timerFired = false;
                var timerId = setTimeout(function() {
                    timerFired = true;
                }, 1);
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let stats = run_event_loop(&ctx, &el).expect("event loop borde lyckas");
            assert!(stats.timers_fired > 0, "Minst en timer borde ha körts");

            let val: bool = ctx.eval("timerFired").expect("borde kunna läsa timerFired");
            assert!(val, "timerFired borde vara true");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_clear_timeout() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var timerFired = false;
                var id = setTimeout(function() {
                    timerFired = true;
                }, 1);
                clearTimeout(id);
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: bool = ctx.eval("timerFired").expect("borde kunna läsa timerFired");
            assert!(!val, "timerFired borde vara false efter clearTimeout");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_set_interval_fires_multiple() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var count = 0;
                var id = setInterval(function() {
                    count++;
                    if (count >= 3) {
                        clearInterval(id);
                    }
                }, 1);
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: i32 = ctx.eval("count").expect("borde kunna läsa count");
            assert!(
                val >= 3,
                "count borde vara >= 3 efter setInterval: got {}",
                val
            );
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_request_animation_frame() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var rafCalled = false;
                var rafTimestamp = 0;
                requestAnimationFrame(function(ts) {
                    rafCalled = true;
                    rafTimestamp = ts;
                });
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let stats = run_event_loop(&ctx, &el);
            assert!(stats.is_ok(), "Event loop borde lyckas");

            let val: bool = ctx.eval("rafCalled").expect("borde kunna läsa rafCalled");
            assert!(val, "rAF callback borde ha körts");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_cancel_animation_frame() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var rafCalled = false;
                var id = requestAnimationFrame(function(ts) {
                    rafCalled = true;
                });
                cancelAnimationFrame(id);
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: bool = ctx.eval("rafCalled").expect("borde kunna läsa rafCalled");
            assert!(!val, "rAF borde inte ha körts efter cancel");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_queue_microtask() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var microtaskRan = false;
                queueMicrotask(function() {
                    microtaskRan = true;
                });
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: bool = ctx
                .eval("microtaskRan")
                .expect("borde kunna läsa microtaskRan");
            assert!(val, "Microtask borde ha körts");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_max_timers_limit() {
        with_quickjs_context(|_rt, ctx, _el| {
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
            let _: Value = ctx.eval(code.as_str()).expect("eval borde lyckas");

            let val: i32 = ctx.eval("lastId").expect("borde kunna läsa lastId");
            assert_eq!(val, -1, "Borde returnera -1 vid timer-limit");
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_event_loop_stats() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                setTimeout(function() {}, 1);
                requestAnimationFrame(function(ts) {});
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let stats = run_event_loop(&ctx, &el).unwrap();
            assert!(stats.ticks > 0, "Borde ha kört minst 1 tick");

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
        });
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
    #[cfg(feature = "js-eval")]
    fn test_promise_chain() {
        with_quickjs_context(|_rt, ctx, el| {
            let code = r#"
                var result = 0;
                Promise.resolve(1)
                    .then(function(v) { return v + 1; })
                    .then(function(v) { return v * 10; })
                    .then(function(v) { result = v; });
            "#;
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: i32 = ctx.eval("result").expect("borde kunna läsa result");
            assert_eq!(val, 20, "Promise-kedja borde ge 20: got {}", val);
        });
    }

    #[test]
    #[cfg(feature = "js-eval")]
    fn test_set_timeout_with_promise() {
        with_quickjs_context(|_rt, ctx, el| {
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
            let _: Value = ctx.eval(code).expect("eval borde lyckas");
            let _ = run_event_loop(&ctx, &el);

            let val: String = ctx.eval("steps.join(',')").expect("borde kunna läsa steps");
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
        });
    }
}
