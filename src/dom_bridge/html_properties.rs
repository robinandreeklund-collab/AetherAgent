// ─── HTML Element Reflected Properties ─────────────────────────────────────
//
// Reflekterar HTML-attribut som IDL-properties per HTML-spec.
// Används av make_element_object för att sätta .type, .value, .checked, etc.

use rquickjs::{Ctx, Object};

use crate::arena_dom::NodeKey;

use super::state::SharedState;

/// Sätter HTML-reflekterade properties på ett element-objekt.
/// Anropas från make_element_object för element-noder (nodeType=1).
pub(super) fn set_html_reflected_properties<'js>(
    ctx: &Ctx<'js>,
    obj: &Object<'js>,
    state: &SharedState,
    key: NodeKey,
) -> rquickjs::Result<()> {
    let s = state.borrow();
    let node = match s.arena.nodes.get(key) {
        Some(n) => n,
        None => return Ok(()),
    };
    let tag = node.tag.as_deref().unwrap_or("");

    // .type — defaultvärde beror på element
    let type_default = match tag {
        "button" => "submit",
        "input" => "text",
        "command" => "command",
        "link" | "script" | "ol" => "",
        _ => "",
    };
    let type_val = node.get_attr("type").unwrap_or(type_default);
    if matches!(
        tag,
        "input"
            | "button"
            | "select"
            | "textarea"
            | "link"
            | "script"
            | "ol"
            | "embed"
            | "object"
            | "source"
            | "style"
            | "command"
            | "menu"
    ) {
        obj.set("type", rquickjs::String::from_str(ctx.clone(), type_val)?)?;
    }

    // .value — reflekterar value-attribut
    if matches!(
        tag,
        "input"
            | "textarea"
            | "select"
            | "option"
            | "button"
            | "param"
            | "li"
            | "meter"
            | "progress"
            | "data"
            | "output"
    ) {
        let val = node.get_attr("value").unwrap_or("");
        obj.set("value", rquickjs::String::from_str(ctx.clone(), val)?)?;
        obj.set(
            "defaultValue",
            rquickjs::String::from_str(ctx.clone(), val)?,
        )?;
    }

    // .name — reflekterar name-attribut
    if matches!(
        tag,
        "input"
            | "button"
            | "select"
            | "textarea"
            | "form"
            | "iframe"
            | "object"
            | "map"
            | "fieldset"
            | "output"
            | "slot"
    ) {
        let name = node.get_attr("name").unwrap_or("");
        obj.set("name", rquickjs::String::from_str(ctx.clone(), name)?)?;
    }

    // Boolean reflected properties
    let disabled = node.has_attr("disabled");
    let checked = node.has_attr("checked");
    let readonly = node.has_attr("readonly");
    let required = node.has_attr("required");
    let multiple = node.has_attr("multiple");
    let autofocus = node.has_attr("autofocus");
    let novalidate = node.has_attr("novalidate");

    if matches!(tag, "input" | "button" | "select" | "textarea" | "fieldset") {
        obj.set("disabled", disabled)?;
    }
    if tag == "input" {
        obj.set("checked", checked)?;
        obj.set("defaultChecked", checked)?;
        obj.set("readOnly", readonly)?;
        obj.set("required", required)?;
        obj.set("multiple", multiple)?;
    }
    if tag == "textarea" {
        obj.set("readOnly", readonly)?;
        obj.set("required", required)?;
    }
    if tag == "select" {
        obj.set("required", required)?;
        obj.set("multiple", multiple)?;
    }
    if matches!(tag, "input" | "button" | "select" | "textarea") {
        obj.set("autofocus", autofocus)?;
    }
    if tag == "form" {
        obj.set("noValidate", novalidate)?;
    }

    // .src, .href, .action — URL-reflecting
    if let Some(src) = node.get_attr("src") {
        if matches!(
            tag,
            "img" | "script" | "iframe" | "audio" | "video" | "source" | "input" | "embed"
        ) {
            obj.set("src", rquickjs::String::from_str(ctx.clone(), src)?)?;
        }
    }
    if let Some(href) = node.get_attr("href") {
        if matches!(tag, "a" | "link" | "area" | "base") {
            obj.set("href", rquickjs::String::from_str(ctx.clone(), href)?)?;
        }
    }
    if let Some(action) = node.get_attr("action") {
        if tag == "form" {
            obj.set("action", rquickjs::String::from_str(ctx.clone(), action)?)?;
        }
    }

    // .placeholder
    if matches!(tag, "input" | "textarea") {
        if let Some(ph) = node.get_attr("placeholder") {
            obj.set("placeholder", rquickjs::String::from_str(ctx.clone(), ph)?)?;
        }
    }

    // .tabIndex
    let tabindex = node
        .get_attr("tabindex")
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(
            if matches!(tag, "a" | "button" | "input" | "select" | "textarea") {
                0
            } else {
                -1
            },
        );
    obj.set("tabIndex", tabindex)?;

    // .title, .lang, .dir
    if let Some(title) = node.get_attr("title") {
        obj.set("title", rquickjs::String::from_str(ctx.clone(), title)?)?;
    }
    if let Some(lang) = node.get_attr("lang") {
        obj.set("lang", rquickjs::String::from_str(ctx.clone(), lang)?)?;
    }
    if let Some(dir) = node.get_attr("dir") {
        obj.set("dir", rquickjs::String::from_str(ctx.clone(), dir)?)?;
    }

    Ok(())
}
