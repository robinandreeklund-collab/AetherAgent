#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLVideoElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLVideoElementGetWidth {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetWidth {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("width"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLVideoElementSetWidth {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementSetWidth {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.get(0).and_then(|v| v.as_int()).unwrap_or(0) as u32;
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("width", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLVideoElementGetHeight {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetHeight {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("height"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLVideoElementSetHeight {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementSetHeight {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.get(0).and_then(|v| v.as_int()).unwrap_or(0) as u32;
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("height", &val.to_string());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLVideoElementGetVideoWidth {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetVideoWidth {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("videowidth"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLVideoElementGetVideoHeight {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetVideoHeight {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("videoheight"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), val))
    }
}

pub(crate) struct HTMLVideoElementGetPoster {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetPoster {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("poster"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLVideoElementSetPoster {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementSetPoster {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("poster", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLVideoElementGetPlaysInline {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementGetPlaysInline {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("playsinline"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLVideoElementSetPlaysInline {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLVideoElementSetPlaysInline {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("playsinline", "");
            } else {
                n.remove_attr("playsinline");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLVideoElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlvideo_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "width",
        Accessor::new(
            JsFn(HTMLVideoElementGetWidth {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLVideoElementSetWidth {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "height",
        Accessor::new(
            JsFn(HTMLVideoElementGetHeight {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLVideoElementSetHeight {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "videoWidth",
        Accessor::new_get(JsFn(HTMLVideoElementGetVideoWidth {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "videoHeight",
        Accessor::new_get(JsFn(HTMLVideoElementGetVideoHeight {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "poster",
        Accessor::new(
            JsFn(HTMLVideoElementGetPoster {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLVideoElementSetPoster {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "playsInline",
        Accessor::new(
            JsFn(HTMLVideoElementGetPlaysInline {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLVideoElementSetPlaysInline {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    Ok(())
}
