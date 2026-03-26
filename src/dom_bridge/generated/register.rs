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
        "video" => super::htmlvideo_element::register_htmlvideo_element(ctx, obj, state, key)?,
        "audio" => super::htmlaudio_element::register_htmlaudio_element(ctx, obj, state, key)?,
        "source" => super::htmlsource_element::register_htmlsource_element(ctx, obj, state, key)?,
        "track" => super::htmltrack_element::register_htmltrack_element(ctx, obj, state, key)?,
        "div" => super::htmldiv_element::register_htmldiv_element(ctx, obj, state, key)?,
        "span" => super::htmlspan_element::register_htmlspan_element(ctx, obj, state, key)?,
        "p" => super::htmlparagraph_element::register_htmlparagraph_element(ctx, obj, state, key)?,
        "h1" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "h2" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "h3" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "h4" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "h5" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "h6" => super::htmlheading_element::register_htmlheading_element(ctx, obj, state, key)?,
        "pre" => super::htmlpre_element::register_htmlpre_element(ctx, obj, state, key)?,
        "blockquote" => super::htmlquote_element::register_htmlquote_element(ctx, obj, state, key)?,
        "q" => super::htmlquote_element::register_htmlquote_element(ctx, obj, state, key)?,
        "ol" => super::htmlolist_element::register_htmlolist_element(ctx, obj, state, key)?,
        "ul" => super::htmlulist_element::register_htmlulist_element(ctx, obj, state, key)?,
        "li" => super::htmllielement::register_htmllielement(ctx, obj, state, key)?,
        "dl" => super::htmldlist_element::register_htmldlist_element(ctx, obj, state, key)?,
        "hr" => super::htmlhrelement::register_htmlhrelement(ctx, obj, state, key)?,
        "br" => super::htmlbrelement::register_htmlbrelement(ctx, obj, state, key)?,
        "table" => super::htmltable_element::register_htmltable_element(ctx, obj, state, key)?,
        "thead" => super::htmltable_section_element::register_htmltable_section_element(
            ctx, obj, state, key,
        )?,
        "tbody" => super::htmltable_section_element::register_htmltable_section_element(
            ctx, obj, state, key,
        )?,
        "tfoot" => super::htmltable_section_element::register_htmltable_section_element(
            ctx, obj, state, key,
        )?,
        "tr" => super::htmltable_row_element::register_htmltable_row_element(ctx, obj, state, key)?,
        "td" => {
            super::htmltable_cell_element::register_htmltable_cell_element(ctx, obj, state, key)?
        }
        "th" => {
            super::htmltable_cell_element::register_htmltable_cell_element(ctx, obj, state, key)?
        }
        "caption" => super::htmltable_caption_element::register_htmltable_caption_element(
            ctx, obj, state, key,
        )?,
        "col" => {
            super::htmltable_col_element::register_htmltable_col_element(ctx, obj, state, key)?
        }
        "colgroup" => {
            super::htmltable_col_element::register_htmltable_col_element(ctx, obj, state, key)?
        }
        "iframe" => super::htmliframe_element::register_htmliframe_element(ctx, obj, state, key)?,
        "embed" => super::htmlembed_element::register_htmlembed_element(ctx, obj, state, key)?,
        "object" => super::htmlobject_element::register_htmlobject_element(ctx, obj, state, key)?,
        "canvas" => super::htmlcanvas_element::register_htmlcanvas_element(ctx, obj, state, key)?,
        "dialog" => super::htmldialog_element::register_htmldialog_element(ctx, obj, state, key)?,
        "details" => {
            super::htmldetails_element::register_htmldetails_element(ctx, obj, state, key)?
        }
        "summary" => {
            super::htmlsummary_element::register_htmlsummary_element(ctx, obj, state, key)?
        }
        "script" => super::htmlscript_element::register_htmlscript_element(ctx, obj, state, key)?,
        "style" => super::htmlstyle_element::register_htmlstyle_element(ctx, obj, state, key)?,
        "link" => super::htmllink_element::register_htmllink_element(ctx, obj, state, key)?,
        "meta" => super::htmlmeta_element::register_htmlmeta_element(ctx, obj, state, key)?,
        "base" => super::htmlbase_element::register_htmlbase_element(ctx, obj, state, key)?,
        "title" => super::htmltitle_element::register_htmltitle_element(ctx, obj, state, key)?,
        "body" => super::htmlbody_element::register_htmlbody_element(ctx, obj, state, key)?,
        "html" => super::htmlhtml_element::register_htmlhtml_element(ctx, obj, state, key)?,
        "head" => super::htmlhead_element::register_htmlhead_element(ctx, obj, state, key)?,
        "area" => super::htmlarea_element::register_htmlarea_element(ctx, obj, state, key)?,
        "map" => super::htmlmap_element::register_htmlmap_element(ctx, obj, state, key)?,
        "data" => super::htmldata_element::register_htmldata_element(ctx, obj, state, key)?,
        "time" => super::htmltime_element::register_htmltime_element(ctx, obj, state, key)?,
        "picture" => {
            super::htmlpicture_element::register_htmlpicture_element(ctx, obj, state, key)?
        }
        "optgroup" => {
            super::htmlopt_group_element::register_htmlopt_group_element(ctx, obj, state, key)?
        }
        "datalist" => {
            super::htmldata_list_element::register_htmldata_list_element(ctx, obj, state, key)?
        }
        "menu" => super::htmlmenu_element::register_htmlmenu_element(ctx, obj, state, key)?,
        "template" => {
            super::htmltemplate_element::register_htmltemplate_element(ctx, obj, state, key)?
        }
        "slot" => super::htmlslot_element::register_htmlslot_element(ctx, obj, state, key)?,
        _ => {}
    }
    Ok(())
}
