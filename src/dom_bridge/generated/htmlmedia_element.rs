#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLMediaElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLMediaElementGetSrc {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetSrc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("src"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementSetSrc {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetSrc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("src", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetCurrentSrc {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetCurrentSrc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("currentsrc"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementSetCurrentSrc {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetCurrentSrc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("currentsrc", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetCrossOrigin {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetCrossOrigin {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("crossorigin"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementSetCrossOrigin {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetCrossOrigin {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("crossorigin", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetNetworkState {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetNetworkState {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("networkstate"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementGetPreload {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetPreload {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("preload"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementSetPreload {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetPreload {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("preload", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetReadyState {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetReadyState {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("readystate"))
            .unwrap_or("");
        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())
    }
}

pub(crate) struct HTMLMediaElementGetSeeking {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetSeeking {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("seeking"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementGetCurrentTime {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetCurrentTime {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("currenttime"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetCurrentTime {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetCurrentTime {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("currenttime", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetDuration {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetDuration {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("duration"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementGetPaused {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetPaused {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("paused"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementGetDefaultPlaybackRate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetDefaultPlaybackRate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("defaultplaybackrate"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetDefaultPlaybackRate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetDefaultPlaybackRate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("defaultplaybackrate", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetPlaybackRate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetPlaybackRate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("playbackrate"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetPlaybackRate {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetPlaybackRate {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("playbackrate", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetEnded {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetEnded {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("ended"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementGetAutoplay {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetAutoplay {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("autoplay"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetAutoplay {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetAutoplay {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("autoplay", "");
            } else {
                n.remove_attr("autoplay");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetLoop {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetLoop {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("loop"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetLoop {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetLoop {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("loop", "");
            } else {
                n.remove_attr("loop");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetControls {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetControls {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("controls"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetControls {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetControls {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("controls", "");
            } else {
                n.remove_attr("controls");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetVolume {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetVolume {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("volume"))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(Value::new_float(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetVolume {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetVolume {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .get(0)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.set_attr("volume", &val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetMuted {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetMuted {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("muted"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetMuted {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetMuted {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("muted", "");
            } else {
                n.remove_attr("muted");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementGetDefaultMuted {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementGetDefaultMuted {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr("defaultmuted"))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), val))
    }
}

pub(crate) struct HTMLMediaElementSetDefaultMuted {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementSetDefaultMuted {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            if val {
                n.set_attr("defaultmuted", "");
            } else {
                n.remove_attr("defaultmuted");
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementPlay {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementPlay {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementPause {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementPause {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementLoad {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementLoad {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(crate) struct HTMLMediaElementCanPlayType {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementCanPlayType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

pub(crate) struct HTMLMediaElementFastSeek {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLMediaElementFastSeek {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Registrera alla HTMLMediaElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmlmedia_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "src",
        Accessor::new(
            JsFn(HTMLMediaElementGetSrc {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetSrc {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "currentSrc",
        Accessor::new(
            JsFn(HTMLMediaElementGetCurrentSrc {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetCurrentSrc {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "crossOrigin",
        Accessor::new(
            JsFn(HTMLMediaElementGetCrossOrigin {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetCrossOrigin {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "networkState",
        Accessor::new_get(JsFn(HTMLMediaElementGetNetworkState {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "preload",
        Accessor::new(
            JsFn(HTMLMediaElementGetPreload {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetPreload {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "readyState",
        Accessor::new_get(JsFn(HTMLMediaElementGetReadyState {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "seeking",
        Accessor::new_get(JsFn(HTMLMediaElementGetSeeking {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "currentTime",
        Accessor::new(
            JsFn(HTMLMediaElementGetCurrentTime {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetCurrentTime {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "duration",
        Accessor::new_get(JsFn(HTMLMediaElementGetDuration {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "paused",
        Accessor::new_get(JsFn(HTMLMediaElementGetPaused {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "defaultPlaybackRate",
        Accessor::new(
            JsFn(HTMLMediaElementGetDefaultPlaybackRate {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetDefaultPlaybackRate {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "playbackRate",
        Accessor::new(
            JsFn(HTMLMediaElementGetPlaybackRate {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetPlaybackRate {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "ended",
        Accessor::new_get(JsFn(HTMLMediaElementGetEnded {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "autoplay",
        Accessor::new(
            JsFn(HTMLMediaElementGetAutoplay {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetAutoplay {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "loop",
        Accessor::new(
            JsFn(HTMLMediaElementGetLoop {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetLoop {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "controls",
        Accessor::new(
            JsFn(HTMLMediaElementGetControls {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetControls {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "volume",
        Accessor::new(
            JsFn(HTMLMediaElementGetVolume {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetVolume {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "muted",
        Accessor::new(
            JsFn(HTMLMediaElementGetMuted {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetMuted {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.prop(
        "defaultMuted",
        Accessor::new(
            JsFn(HTMLMediaElementGetDefaultMuted {
                state: Rc::clone(state),
                key,
            }),
            JsFn(HTMLMediaElementSetDefaultMuted {
                state: Rc::clone(state),
                key,
            }),
        ),
    )?;
    obj.set(
        "play",
        Function::new(
            ctx.clone(),
            JsFn(HTMLMediaElementPlay {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "pause",
        Function::new(
            ctx.clone(),
            JsFn(HTMLMediaElementPause {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "load",
        Function::new(
            ctx.clone(),
            JsFn(HTMLMediaElementLoad {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "canPlayType",
        Function::new(
            ctx.clone(),
            JsFn(HTMLMediaElementCanPlayType {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "fastSeek",
        Function::new(
            ctx.clone(),
            JsFn(HTMLMediaElementFastSeek {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
