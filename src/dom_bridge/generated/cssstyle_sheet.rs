#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: CSSStyleSheet
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct CSSStyleSheetGetOwnerRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetGetOwnerRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("ownerrule"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleSheetGetCssRules {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetGetCssRules {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("cssrules"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleSheetInsertRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetInsertRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct CSSStyleSheetDeleteRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetDeleteRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleSheetReplace {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetReplace {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleSheetReplaceSync {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleSheetReplaceSync {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla CSSStyleSheet-properties och metoder på ett JS-objekt.
pub(crate) fn register_cssstyle_sheet<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "ownerRule",
        Accessor::new_get(JsFn(CSSStyleSheetGetOwnerRule {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "cssRules",
        Accessor::new_get(JsFn(CSSStyleSheetGetCssRules {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.set(
        "insertRule",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleSheetInsertRule {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "deleteRule",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleSheetDeleteRule {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replace",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleSheetReplace {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replaceSync",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleSheetReplaceSync {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
