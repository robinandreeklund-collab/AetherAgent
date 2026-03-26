#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLCanvasElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLCanvasElementGetWidth {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLCanvasElementGetWidth {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("width"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLCanvasElementSetWidth {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLCanvasElementSetWidth {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("width", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLCanvasElementGetHeight {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLCanvasElementGetHeight {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("height"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLCanvasElementSetHeight {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLCanvasElementSetHeight {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("height", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLCanvasElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlcanvas_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "width",
        Accessor::new(
            JsFn(HTMLCanvasElementGetWidth {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLCanvasElementSetWidth {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "height",
        Accessor::new(
            JsFn(HTMLCanvasElementGetHeight {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLCanvasElementSetHeight {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    Ok(())
}
