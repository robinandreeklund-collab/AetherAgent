#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLMeterElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLMeterElementGetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("value"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("value", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetMin {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetMin {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("min"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetMin {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetMin {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("min", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetMax {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetMax {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("max"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetMax {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetMax {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("max", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetLow {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetLow {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("low"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetLow {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetLow {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("low", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetHigh {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetHigh {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("high"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetHigh {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetHigh {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("high", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetOptimum {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetOptimum {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("optimum"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMeterElementSetOptimum {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementSetOptimum {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("optimum", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMeterElementGetLabels {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMeterElementGetLabels {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("labels"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

/// Registrera alla HTMLMeterElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlmeter_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "value",
        Accessor::new(
            JsFn(HTMLMeterElementGetValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetValue {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "min",
        Accessor::new(
            JsFn(HTMLMeterElementGetMin {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetMin {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "max",
        Accessor::new(
            JsFn(HTMLMeterElementGetMax {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetMax {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "low",
        Accessor::new(
            JsFn(HTMLMeterElementGetLow {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetLow {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "high",
        Accessor::new(
            JsFn(HTMLMeterElementGetHigh {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetHigh {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "optimum",
        Accessor::new(
            JsFn(HTMLMeterElementGetOptimum {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMeterElementSetOptimum {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "labels",
        Accessor::new_get(JsFn(HTMLMeterElementGetLabels {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    Ok(())
}
