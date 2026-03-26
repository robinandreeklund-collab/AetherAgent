#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLAnchorElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLAnchorElementGetTarget {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetTarget {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("target"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetTarget {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetTarget {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("target", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetDownload {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetDownload {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("download"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetDownload {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetDownload {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("download", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetPing {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetPing {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("ping"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetPing {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetPing {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("ping", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetRel {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetRel {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("rel"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetRel {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetRel {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("rel", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetHreflang {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetHreflang {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("hreflang"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetHreflang {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetHreflang {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("hreflang", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetType {
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

pub(crate) struct HTMLAnchorElementSetType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetType {
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

pub(crate) struct HTMLAnchorElementGetText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("text"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetText {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("text", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetReferrerPolicy {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetReferrerPolicy {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("referrerpolicy"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetReferrerPolicy {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetReferrerPolicy {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("referrerpolicy", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetHref {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetHref {
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

pub(crate) struct HTMLAnchorElementSetHref {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetHref {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("href", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetOrigin {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetOrigin {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_origin(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementGetProtocol {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetProtocol {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_protocol(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetProtocol {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetProtocol {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetUsername {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetUsername {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_username(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetUsername {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetUsername {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetPassword {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetPassword {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_password(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetPassword {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetPassword {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetHost {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetHost {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_host(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetHost {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetHost {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetHostname {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetHostname {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_hostname(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetHostname {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetHostname {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetPort {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetPort {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_port(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetPort {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetPort {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetPathname {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetPathname {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_pathname(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetPathname {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetPathname {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetSearch {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetSearch {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_search(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetSearch {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetSearch {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLAnchorElementGetHash {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementGetHash {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_url_hash(&s, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(crate) struct HTMLAnchorElementSetHash {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLAnchorElementSetHash {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLAnchorElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlanchor_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "target",
        Accessor::new(
            JsFn(HTMLAnchorElementGetTarget {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetTarget {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "download",
        Accessor::new(
            JsFn(HTMLAnchorElementGetDownload {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetDownload {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "ping",
        Accessor::new(
            JsFn(HTMLAnchorElementGetPing {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetPing {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "rel",
        Accessor::new(
            JsFn(HTMLAnchorElementGetRel {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetRel {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "hreflang",
        Accessor::new(
            JsFn(HTMLAnchorElementGetHreflang {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetHreflang {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "type",
        Accessor::new(
            JsFn(HTMLAnchorElementGetType {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetType {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "text",
        Accessor::new(
            JsFn(HTMLAnchorElementGetText {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetText {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "referrerPolicy",
        Accessor::new(
            JsFn(HTMLAnchorElementGetReferrerPolicy {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetReferrerPolicy {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "href",
        Accessor::new(
            JsFn(HTMLAnchorElementGetHref {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetHref {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "origin",
        Accessor::new_get(JsFn(HTMLAnchorElementGetOrigin {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "protocol",
        Accessor::new(
            JsFn(HTMLAnchorElementGetProtocol {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetProtocol {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "username",
        Accessor::new(
            JsFn(HTMLAnchorElementGetUsername {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetUsername {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "password",
        Accessor::new(
            JsFn(HTMLAnchorElementGetPassword {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetPassword {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "host",
        Accessor::new(
            JsFn(HTMLAnchorElementGetHost {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetHost {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "hostname",
        Accessor::new(
            JsFn(HTMLAnchorElementGetHostname {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetHostname {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "port",
        Accessor::new(
            JsFn(HTMLAnchorElementGetPort {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetPort {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "pathname",
        Accessor::new(
            JsFn(HTMLAnchorElementGetPathname {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetPathname {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "search",
        Accessor::new(
            JsFn(HTMLAnchorElementGetSearch {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetSearch {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "hash",
        Accessor::new(
            JsFn(HTMLAnchorElementGetHash {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLAnchorElementSetHash {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    Ok(())
}
