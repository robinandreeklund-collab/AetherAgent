#![allow(unused_imports, dead_code, unused_variables, clippy::all)]
// Auto-genererat från WebIDL: HTMLTableRowElement
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;
use rquickjs::{Ctx, Value};

pub(crate) struct HTMLTableRowElementGetRowIndex {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLTableRowElementGetRowIndex {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_row_index(&s, self.key);
        Ok(Value::new_int(ctx.clone(), val as i32))
    }
}

pub(crate) struct HTMLTableRowElementGetSectionRowIndex {
    pub(crate) state: SharedState,
    pub(crate) key: NodeKey,
}
impl JsHandler for HTMLTableRowElementGetSectionRowIndex {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = super::super::computed::compute_section_row_index(&s, self.key);
        Ok(Value::new_int(ctx.clone(), val as i32))
    }
}

/// Registrera alla HTMLTableRowElement-properties och metoder på ett JS-objekt.
pub(crate) fn register_htmltable_row_element<'js>(
    ctx: &Ctx<'js>,
    obj: &rquickjs::Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    use crate::event_loop::JsFn;
    use rquickjs::{object::Accessor, Function};
    use std::rc::Rc;

    obj.prop(
        "rowIndex",
        Accessor::new_get(JsFn(HTMLTableRowElementGetRowIndex {
            state: Rc::clone(state),
            key,
        })),
    )?;
    obj.prop(
        "sectionRowIndex",
        Accessor::new_get(JsFn(HTMLTableRowElementGetSectionRowIndex {
            state: Rc::clone(state),
            key,
        })),
    )?;
    Ok(())
}
