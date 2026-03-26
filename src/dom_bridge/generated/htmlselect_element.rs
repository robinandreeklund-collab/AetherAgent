#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLSelectElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLSelectElementGetAutocomplete {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetAutocomplete {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("autocomplete"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLSelectElementSetAutocomplete {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetAutocomplete {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("autocomplete", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetDisabled {
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

pub(crate) struct HTMLSelectElementSetDisabled {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetDisabled {
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

pub(crate) struct HTMLSelectElementGetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetLength {
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

pub(crate) struct HTMLSelectElementSetLength {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetLength {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.get(0).and_then(|v| v.as_int()).unwrap_or(0) as i32;
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("length", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetMultiple {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetMultiple {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("multiple"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLSelectElementSetMultiple {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetMultiple {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("multiple", "");
            } else {
                n.remove_attr("multiple");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetName {
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

pub(crate) struct HTMLSelectElementSetName {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetName {
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

pub(crate) struct HTMLSelectElementGetRequired {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetRequired {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("required"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLSelectElementSetRequired {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetRequired {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("required", "");
            } else {
                n.remove_attr("required");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetSelectedIndex {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetSelectedIndex {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("selectedindex"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(-1);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLSelectElementSetSelectedIndex {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetSelectedIndex {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.get(0).and_then(|v| v.as_int()).unwrap_or(0) as i32;
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("selectedindex", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetSize {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetSize {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("size"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLSelectElementSetSize {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetSize {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.get(0).and_then(|v| v.as_int()).unwrap_or(0) as u32;
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("size", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("value"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLSelectElementSetValue {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("value", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLSelectElementGetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("select-one");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLSelectElementGetWillValidate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetWillValidate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("willvalidate"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLSelectElementGetValidationMessage {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetValidationMessage {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("validationmessage"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLSelectElementGetLabels {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementGetLabels {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("labels"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLSelectElementCheckValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementCheckValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

pub(crate) struct HTMLSelectElementReportValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementReportValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_bool(ctx.clone(), true))
    }
}

pub(crate) struct HTMLSelectElementSetCustomValidity {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLSelectElementSetCustomValidity {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // TODO: Implementera HTMLSelectElement.setCustomValidity()
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLSelectElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlselect_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "autocomplete",
        Accessor::new(
            JsFn(HTMLSelectElementGetAutocomplete {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetAutocomplete {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "disabled",
        Accessor::new(
            JsFn(HTMLSelectElementGetDisabled {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetDisabled {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "length",
        Accessor::new(
            JsFn(HTMLSelectElementGetLength {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetLength {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "multiple",
        Accessor::new(
            JsFn(HTMLSelectElementGetMultiple {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetMultiple {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "name",
        Accessor::new(
            JsFn(HTMLSelectElementGetName {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetName {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "required",
        Accessor::new(
            JsFn(HTMLSelectElementGetRequired {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetRequired {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "selectedIndex",
        Accessor::new(
            JsFn(HTMLSelectElementGetSelectedIndex {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetSelectedIndex {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "size",
        Accessor::new(
            JsFn(HTMLSelectElementGetSize {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetSize {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "value",
        Accessor::new(
            JsFn(HTMLSelectElementGetValue {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLSelectElementSetValue {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "type",
        Accessor::new_get(JsFn(HTMLSelectElementGetType {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "willValidate",
        Accessor::new_get(JsFn(HTMLSelectElementGetWillValidate {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "validationMessage",
        Accessor::new_get(JsFn(HTMLSelectElementGetValidationMessage {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "labels",
        Accessor::new_get(JsFn(HTMLSelectElementGetLabels {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.set(
        "checkValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLSelectElementCheckValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "reportValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLSelectElementReportValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setCustomValidity",
        Function::new(
            ctx.clone(),
            JsFn(HTMLSelectElementSetCustomValidity {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
