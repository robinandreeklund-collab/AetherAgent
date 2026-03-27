// ─── Form Association ────────────────────────────────────────────────────────
//
// Implementerar form.elements (HTMLFormControlsCollection), form.reset(),
// och form owner association.
//
// Spec: https://html.spec.whatwg.org/multipage/forms.html#the-form-element

use crate::arena_dom::NodeKey;

use super::super::state::BridgeState;

/// Hitta alla form controls vars form-owner är `form_key`.
/// Returnerar NodeKeys i tree order.
pub(in crate::dom_bridge) fn get_form_elements(
    state: &BridgeState,
    form_key: NodeKey,
) -> Vec<NodeKey> {
    let mut elements = vec![];
    collect_form_controls(state, form_key, form_key, &mut elements);
    elements
}

fn collect_form_controls(
    state: &BridgeState,
    form_key: NodeKey,
    current: NodeKey,
    results: &mut Vec<NodeKey>,
) {
    let node = match state.arena.nodes.get(current) {
        Some(n) => n,
        None => return,
    };

    // Kontrollera om detta element är ett form control
    if current != form_key {
        let is_control = matches!(
            node.tag.as_deref(),
            Some("input" | "select" | "textarea" | "button" | "output" | "fieldset" | "object")
        );
        if is_control {
            // Kontrollera form owner:
            // 1. Explicit form= attribut → matchar form_key?
            // 2. Annars: närmaste <form> ancestor == form_key?
            let owner = super::super::computed::find_form_owner(state, current);
            if owner == Some(form_key) {
                // Exkludera input type=image (per spec: inte listed)
                let is_image =
                    node.tag.as_deref() == Some("input") && node.get_attr("type") == Some("image");
                if !is_image {
                    results.push(current);
                }
            }
        }
    }

    // Rekursera — inklusive inne i form-elementet
    for &child in &node.children {
        collect_form_controls(state, form_key, child, results);
    }
}

/// form.reset() — rensa dirty flags för alla form controls
pub(in crate::dom_bridge) fn reset_form(state: &mut BridgeState, form_key: NodeKey) {
    let elements = get_form_elements(state, form_key);
    for element_key in elements {
        let key_bits = super::super::node_key_to_f64(element_key) as u64;
        if let Some(es) = state.element_state.get_mut(&key_bits) {
            super::input_value::reset_element_state(es);
        }
    }
}

/// form.submit() — i headless mode, samla form data som key=value pairs
#[allow(dead_code)]
pub(in crate::dom_bridge) fn collect_form_data(
    state: &BridgeState,
    form_key: NodeKey,
) -> Vec<(String, String)> {
    let elements = get_form_elements(state, form_key);
    let mut data = vec![];

    for elem_key in elements {
        let node = match state.arena.nodes.get(elem_key) {
            Some(n) => n,
            None => continue,
        };
        let tag = node.tag.as_deref().unwrap_or("");

        // Skippa disabled elements
        if node.has_attr("disabled") {
            continue;
        }

        let name = match node.get_attr("name") {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };

        match tag {
            "input" => {
                let input_type = node.get_attr("type").unwrap_or("text");
                match input_type {
                    "checkbox" | "radio" => {
                        let checked = super::input_value::get_input_checked(state, elem_key);
                        if checked {
                            let value = node.get_attr("value").unwrap_or("on");
                            data.push((name, value.to_string()));
                        }
                    }
                    "file" | "image" | "reset" | "button" => {
                        // Skippa i form submission
                    }
                    "submit" => {
                        // Bara om det är submit-knappen (inte hanterbart utan eventinfo)
                    }
                    _ => {
                        let value = super::input_value::get_input_value(state, elem_key);
                        data.push((name, value));
                    }
                }
            }
            "select" => {
                let value = super::super::computed::get_effective_value(state, elem_key);
                data.push((name, value));
            }
            "textarea" => {
                let value = super::super::computed::get_effective_value(state, elem_key);
                data.push((name, value));
            }
            _ => {}
        }
    }

    data
}
