#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLDialogElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLDialogElementGetOpen {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementGetOpen {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("open"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLDialogElementSetOpen {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementSetOpen {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("open", "");
            } else {
                n.remove_attr("open");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLDialogElementGetReturnValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementGetReturnValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("returnvalue"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLDialogElementSetReturnValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementSetReturnValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("returnvalue", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLDialogElementShow {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementShow {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLDialogElementShowModal {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementShowModal {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLDialogElementClose {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLDialogElementClose {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLDialogElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmldialog_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "open",
        Accessor::new(
            JsFn(HTMLDialogElementGetOpen {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLDialogElementSetOpen {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "returnValue",
        Accessor::new(
            JsFn(HTMLDialogElementGetReturnValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLDialogElementSetReturnValue {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.set(
        "show",
        Function::new(
            ctx.clone(),
            JsFn(HTMLDialogElementShow {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "showModal",
        Function::new(
            ctx.clone(),
            JsFn(HTMLDialogElementShowModal {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "close",
        Function::new(
            ctx.clone(),
            JsFn(HTMLDialogElementClose {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
