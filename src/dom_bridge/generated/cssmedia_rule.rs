#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: CSSMediaRule
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct CSSMediaRuleGetMedia {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSMediaRuleGetMedia {
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

pub(crate) struct CSSMediaRuleGetCssRules {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSMediaRuleGetCssRules {
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

pub(crate) struct CSSMediaRuleInsertRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSMediaRuleInsertRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct CSSMediaRuleDeleteRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSMediaRuleDeleteRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla CSSMediaRule-properties och metoder på ett JS-objekt.
pub(crate) fn register_cssmedia_rule<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "media",
        Accessor::new_get(JsFn(CSSMediaRuleGetMedia {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "cssRules",
        Accessor::new_get(JsFn(CSSMediaRuleGetCssRules {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.set(
        "insertRule",
        Function::new(
            ctx.clone(),
            JsFn(CSSMediaRuleInsertRule {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "deleteRule",
        Function::new(
            ctx.clone(),
            JsFn(CSSMediaRuleDeleteRule {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
