#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLProgressElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLProgressElementGetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementGetValue {
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

pub(crate) struct HTMLProgressElementSetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementSetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_float())
            .or_else(|| args.get(0).and_then(|v| v.as_int().map(|i| i as f64)))
            .unwrap_or(0.0);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("value", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLProgressElementGetMax {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementGetMax {
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

pub(crate) struct HTMLProgressElementSetMax {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementSetMax {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_float())
            .or_else(|| args.get(0).and_then(|v| v.as_int().map(|i| i as f64)))
            .unwrap_or(0.0);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("max", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLProgressElementGetPosition {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementGetPosition {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("position"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(-1.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLProgressElementGetLabels {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLProgressElementGetLabels {
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

/// Registrera alla HTMLProgressElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlprogress_element<'js>(
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
            JsFn(HTMLProgressElementGetValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLProgressElementSetValue {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "max",
        Accessor::new(
            JsFn(HTMLProgressElementGetMax {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLProgressElementSetMax {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "position",
        Accessor::new_get(JsFn(HTMLProgressElementGetPosition {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "labels",
        Accessor::new_get(JsFn(HTMLProgressElementGetLabels {
            state: Rc::clone(state),
            key,
        })),
    )?;
    Ok(())
}
