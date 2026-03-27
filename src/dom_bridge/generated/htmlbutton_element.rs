#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLButtonElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLButtonElementGetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetDisabled {
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

pub(crate) struct HTMLButtonElementSetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetDisabled {
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

pub(crate) struct HTMLButtonElementGetFormAction {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetFormAction {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("formaction"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetFormAction {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetFormAction {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("formaction", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetFormEnctype {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetFormEnctype {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("formenctype"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetFormEnctype {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetFormEnctype {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("formenctype", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetFormMethod {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetFormMethod {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("formmethod"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetFormMethod {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetFormMethod {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("formmethod", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetFormNoValidate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetFormNoValidate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("formnovalidate"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLButtonElementSetFormNoValidate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetFormNoValidate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("formnovalidate", "");
            } else {
                n.remove_attr("formnovalidate");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetFormTarget {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetFormTarget {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("formtarget"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetFormTarget {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetFormTarget {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("formtarget", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetName {
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

pub(crate) struct HTMLButtonElementSetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetName {
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

pub(crate) struct HTMLButtonElementGetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("submit");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("type", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let key_bits = super::super::node_key_to_f64(self.key) as u64;
        let val = s
            .element_state
            .get(&key_bits)
            .and_then(|es| es.value.as_deref())
            .or_else(|| {
                s.arena
                    .nodes
                    .get(self.key)
                    .and_then(|n| n.get_attr("value"))
            })
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementSetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        let key_bits = super::super::node_key_to_f64(self.key) as u64;
        let es = s.element_state.entry(key_bits).or_default();
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        es.value = Some(val);
        es.value_dirty = true;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLButtonElementGetWillValidate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetWillValidate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_will_validate(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLButtonElementGetValidationMessage {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetValidationMessage {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::get_validation_message(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLButtonElementGetLabels {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementGetLabels {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let labels = super::super::computed::find_labels(&s, self.key);
        drop(s);
        let arr = rquickjs::Array::new(ctx.clone())?;
        for (i, lk) in labels.iter().enumerate() {
            let el = super::super::make_element_object(ctx, *lk, &self.state)?;
            arr.set(i, el)?;
        }
        Ok(arr.into_value())
    }
}

pub(crate) struct HTMLButtonElementCheckValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementCheckValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::check_validity(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLButtonElementReportValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementReportValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::check_validity(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLButtonElementSetCustomValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLButtonElementSetCustomValidity {
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

/// Registrera alla HTMLButtonElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlbutton_element<'js>(
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
            JsFn(HTMLButtonElementGetDisabled {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetDisabled {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "formAction",
        Accessor::new(
            JsFn(HTMLButtonElementGetFormAction {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetFormAction {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "formEnctype",
        Accessor::new(
            JsFn(HTMLButtonElementGetFormEnctype {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetFormEnctype {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "formMethod",
        Accessor::new(
            JsFn(HTMLButtonElementGetFormMethod {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetFormMethod {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "formNoValidate",
        Accessor::new(
            JsFn(HTMLButtonElementGetFormNoValidate {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetFormNoValidate {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "formTarget",
        Accessor::new(
            JsFn(HTMLButtonElementGetFormTarget {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetFormTarget {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "name",
        Accessor::new(
            JsFn(HTMLButtonElementGetName {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetName {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "type",
        Accessor::new(
            JsFn(HTMLButtonElementGetType {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetType {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "value",
        Accessor::new(
            JsFn(HTMLButtonElementGetValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLButtonElementSetValue {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "willValidate",
        Accessor::new_get(JsFn(HTMLButtonElementGetWillValidate {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "validationMessage",
        Accessor::new_get(JsFn(HTMLButtonElementGetValidationMessage {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "labels",
        Accessor::new_get(JsFn(HTMLButtonElementGetLabels {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.set(
        "checkValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLButtonElementCheckValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "reportValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLButtonElementReportValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setCustomValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLButtonElementSetCustomValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
