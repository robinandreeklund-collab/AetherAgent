#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: StyleSheet
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct StyleSheetGetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetHref {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetHref {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("href"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetOwnerNode {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetOwnerNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("ownernode"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetParentStyleSheet {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetParentStyleSheet {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("parentstylesheet"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetTitle {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetTitle {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("title"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetMedia {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetMedia {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("media"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct StyleSheetGetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetGetDisabled {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("disabled"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct StyleSheetSetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for StyleSheetSetDisabled {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("disabled", "");
            } else {
                n.remove_attr("disabled");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla StyleSheet-properties och metoder på ett JS-objekt.
pub(crate) fn register_style_sheet<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "type",
        Accessor::new_get(JsFn(StyleSheetGetType {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "href",
        Accessor::new_get(JsFn(StyleSheetGetHref {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "ownerNode",
        Accessor::new_get(JsFn(StyleSheetGetOwnerNode {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "parentStyleSheet",
        Accessor::new_get(JsFn(StyleSheetGetParentStyleSheet {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "title",
        Accessor::new_get(JsFn(StyleSheetGetTitle {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "media",
        Accessor::new_get(JsFn(StyleSheetGetMedia {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "disabled",
        Accessor::new(
            JsFn(StyleSheetGetDisabled {
                state: Rc::clone(state),
                key,
            }),
            JsFn(StyleSheetSetDisabled {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    Ok(())
}
