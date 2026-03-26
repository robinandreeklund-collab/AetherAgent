#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: CSSStyleDeclaration
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct CSSStyleDeclarationGetCssText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetCssText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("csstext"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationSetCssText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationSetCssText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("csstext", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleDeclarationGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetLength {
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

pub(crate) struct CSSStyleDeclarationGetParentRule {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetParentRule {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("parentrule"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationGetCssFloat {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetCssFloat {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("cssfloat"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationSetCssFloat {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationSetCssFloat {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("cssfloat", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleDeclarationItem {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationGetPropertyValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetPropertyValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationGetPropertyPriority {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationGetPropertyPriority {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct CSSStyleDeclarationSetProperty {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationSetProperty {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct CSSStyleDeclarationRemoveProperty {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for CSSStyleDeclarationRemoveProperty {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

/// Registrera alla CSSStyleDeclaration-properties och metoder på ett JS-objekt.
pub(crate) fn register_cssstyle_declaration<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "cssText",
        Accessor::new(
            JsFn(CSSStyleDeclarationGetCssText {
                state: Rc::clone(state),
                key,
            }),
            JsFn(CSSStyleDeclarationSetCssText {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "length",
        Accessor::new_get(JsFn(CSSStyleDeclarationGetLength {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "parentRule",
        Accessor::new_get(JsFn(CSSStyleDeclarationGetParentRule {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "cssFloat",
        Accessor::new(
            JsFn(CSSStyleDeclarationGetCssFloat {
                state: Rc::clone(state),
                key,
            }),
            JsFn(CSSStyleDeclarationSetCssFloat {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleDeclarationItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getPropertyValue",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleDeclarationGetPropertyValue {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getPropertyPriority",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleDeclarationGetPropertyPriority {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setProperty",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleDeclarationSetProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeProperty",
        Function::new(
            ctx.clone(),
            JsFn(CSSStyleDeclarationRemoveProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
