#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: CSSStyleRule
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct CSSStyleRuleGetSelectorText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleRuleGetSelectorText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("selectortext"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleRuleSetSelectorText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleRuleSetSelectorText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("selectortext", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleRuleGetStyle {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleRuleGetStyle {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

/// Registrera alla CSSStyleRule-properties och metoder på ett JS-objekt.
pub(crate) fn register_cssstyle_rule<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "selectorText",
        Accessor::new(
            JsFn(CSSStyleRuleGetSelectorText {
                state: Rc::clone(state),
                key,
            }),
            JsFn(CSSStyleRuleSetSelectorText {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "style",
        Accessor::new_get(JsFn(CSSStyleRuleGetStyle {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    Ok(())
}
