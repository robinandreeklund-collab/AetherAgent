// Auto-genererad master-registreringsfunktion
// REDIGERA INTE — genereras av codegen.

use super::super::state::SharedState;
use crate::arena_dom::NodeKey;
use rquickjs::{Ctx, Object};

/// Registrera auto-genererade HTML element properties baserat på tag-namn.
pub(crate) fn register_html_element_properties<'js>(
    ctx: &Ctx<'js>,
    obj: &Object<'js>,
    state: &SharedState,
    key: NodeKey,
    tag: &str,
) -> rquickjs::Result<()> {
    match tag {
        "input" => super::htmlinput_element::register_htmlinput_element(ctx, obj, state, key)?,
        "button" => super::htmlbutton_element::register_htmlbutton_element(ctx, obj, state, key)?,
        "select" => super::htmlselect_element::register_htmlselect_element(ctx, obj, state, key)?,
        "textarea" => {
            super::htmltext_area_element::register_htmltext_area_element(ctx, obj, state, key)?
        }
        "form" => super::htmlform_element::register_htmlform_element(ctx, obj, state, key)?,
        "a" => super::htmlanchor_element::register_htmlanchor_element(ctx, obj, state, key)?,
        "img" => super::htmlimage_element::register_htmlimage_element(ctx, obj, state, key)?,
        "option" => super::htmloption_element::register_htmloption_element(ctx, obj, state, key)?,
        "label" => super::htmllabel_element::register_htmllabel_element(ctx, obj, state, key)?,
        "fieldset" => {
            super::htmlfield_set_element::register_htmlfield_set_element(ctx, obj, state, key)?
        }
        "output" => super::htmloutput_element::register_htmloutput_element(ctx, obj, state, key)?,
        "legend" => super::htmllegend_element::register_htmllegend_element(ctx, obj, state, key)?,
        "progress" => {
            super::htmlprogress_element::register_htmlprogress_element(ctx, obj, state, key)?
        }
        "meter" => super::htmlmeter_element::register_htmlmeter_element(ctx, obj, state, key)?,
        _ => {}
    }
    Ok(())
}
