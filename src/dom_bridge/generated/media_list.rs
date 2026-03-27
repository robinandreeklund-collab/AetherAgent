#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: MediaList
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct MediaListGetMediaText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListGetMediaText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("mediatext"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct MediaListSetMediaText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListSetMediaText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("mediatext", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct MediaListGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListGetLength {
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

pub(crate) struct MediaListItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct MediaListAppendMedium {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListAppendMedium {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct MediaListDeleteMedium {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for MediaListDeleteMedium {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla MediaList-properties och metoder på ett JS-objekt.
pub(crate) fn register_media_list<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "mediaText",
        Accessor::new(
            JsFn(MediaListGetMediaText {
                state: Rc::clone(state),
                key,
            }),
            JsFn(MediaListSetMediaText {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "length",
        Accessor::new_get(JsFn(MediaListGetLength {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(MediaListItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "appendMedium",
        Function::new(
            ctx.clone(),
            JsFn(MediaListAppendMedium {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "deleteMedium",
        Function::new(
            ctx.clone(),
            JsFn(MediaListDeleteMedium {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
