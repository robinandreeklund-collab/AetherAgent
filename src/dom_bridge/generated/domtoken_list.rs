#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: DOMTokenList
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct DOMTokenListGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListGetLength {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("length"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct DOMTokenListGetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListGetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("value"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct DOMTokenListSetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListSetValue {
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

pub(crate) struct DOMTokenListItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct DOMTokenListContains {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListContains {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

pub(crate) struct DOMTokenListAdd {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListAdd {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct DOMTokenListRemove {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListRemove {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct DOMTokenListToggle {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListToggle {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

pub(crate) struct DOMTokenListReplace {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListReplace {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

pub(crate) struct DOMTokenListSupports {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for DOMTokenListSupports {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

/// Registrera alla DOMTokenList-properties och metoder på ett JS-objekt.
pub(crate) fn register_domtoken_list<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "length",
        Accessor::new_get(JsFn(DOMTokenListGetLength {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "value",
        Accessor::new(
            JsFn(DOMTokenListGetValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(DOMTokenListSetValue {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "contains",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListContains {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "add",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListAdd {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "remove",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListRemove {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "toggle",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListToggle {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replace",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListReplace {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "supports",
        Function::new(
            ctx.clone(),
            JsFn(DOMTokenListSupports {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
