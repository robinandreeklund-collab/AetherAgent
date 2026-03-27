#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: NamedNodeMap
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct NamedNodeMapGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapGetLength {
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

pub(crate) struct NamedNodeMapItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapGetNamedItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapGetNamedItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapGetNamedItemNS {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapGetNamedItemNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapSetNamedItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapSetNamedItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapSetNamedItemNS {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapSetNamedItemNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapRemoveNamedItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapRemoveNamedItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct NamedNodeMapRemoveNamedItemNS {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for NamedNodeMapRemoveNamedItemNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

/// Registrera alla NamedNodeMap-properties och metoder på ett JS-objekt.
pub(crate) fn register_named_node_map<'js>(
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
        Accessor::new_get(JsFn(NamedNodeMapGetLength {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getNamedItem",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapGetNamedItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getNamedItemNS",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapGetNamedItemNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setNamedItem",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapSetNamedItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setNamedItemNS",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapSetNamedItemNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeNamedItem",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapRemoveNamedItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeNamedItemNS",
        Function::new(
            ctx.clone(),
            JsFn(NamedNodeMapRemoveNamedItemNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
