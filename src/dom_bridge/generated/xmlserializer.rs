#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: XMLSerializer
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct XMLSerializerSerializeToString {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for XMLSerializerSerializeToString {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
    }
}

/// Registrera alla XMLSerializer-properties och metoder på ett JS-objekt.
pub(crate) fn register_xmlserializer<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.set(
        "serializeToString",
        Function::new(
            ctx.clone(),
            JsFn(XMLSerializerSerializeToString {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    Ok(())
}
