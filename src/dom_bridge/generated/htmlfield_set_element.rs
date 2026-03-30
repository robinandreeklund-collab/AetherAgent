#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLFieldSetElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLFieldSetElementGetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementGetDisabled {
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

pub(crate) struct HTMLFieldSetElementSetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementSetDisabled {
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

pub(crate) struct HTMLFieldSetElementGetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementGetName {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("name"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLFieldSetElementSetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementSetName {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("name", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLFieldSetElementGetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementGetType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("fieldset");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLFieldSetElementCheckValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementCheckValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::check_validity(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLFieldSetElementReportValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementReportValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::check_validity(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLFieldSetElementSetCustomValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLFieldSetElementSetCustomValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let msg = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let key_bits = super::super::node_key_to_f64(self.key) as u64;
        s.element_state.entry(key_bits).or_default().custom_validity = msg;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLFieldSetElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlfield_set_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "disabled",
        Accessor::new(
            JsFn(HTMLFieldSetElementGetDisabled {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLFieldSetElementSetDisabled {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "name",
        Accessor::new(
            JsFn(HTMLFieldSetElementGetName {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLFieldSetElementSetName {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable(),
    )?;
    obj.prop(
        "type",
        Accessor::new_get(JsFn(HTMLFieldSetElementGetType {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.set(
        "checkValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLFieldSetElementCheckValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "reportValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLFieldSetElementReportValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setCustomValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLFieldSetElementSetCustomValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
