#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: CSSRuleList
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct CSSRuleListGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSRuleListGetLength {
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

pub(crate) struct CSSRuleListItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSRuleListItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

/// Registrera alla CSSRuleList-properties och metoder på ett JS-objekt.
pub(crate) fn register_cssrule_list<'js>(
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
        Accessor::new_get(JsFn(CSSRuleListGetLength {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(CSSRuleListItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
