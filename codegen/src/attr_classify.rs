//! Attribute Classification — bestämmer hur varje WebIDL-attribut genereras.
//!
//! Tre kategorier:
//! - Reflected: mappar direkt till HTML content attribute (get_attr/set_attr)
//! - StateBacked: läser/skriver ElementState i BridgeState
//! - Computed: anropar en Rust-funktion i computed.rs

#[derive(Debug, Clone)]
pub enum AttrCategory {
    /// Mappar till HTML content attribute
    Reflected { html_attr: String },
    /// Separat intern state (value, checked, etc.)
    StateBacked { state_field: String },
    /// Beräknad egenskap — anropar funktion
    Computed { compute_fn: String },
}

#[derive(Debug, Clone)]
pub enum OpCategory {
    /// Tom stub (returnerar default)
    Stub,
    /// Anropar beräkningsfunktion
    ComputedCall { compute_fn: String },
    /// Skriver till element_state
    StateMutator { state_field: String },
}

/// Klassificera ett attribut baserat på interface + attributnamn
pub fn classify(
    interface: &str,
    attr_name: &str,
    _idl_type: &str,
    _readonly: bool,
) -> AttrCategory {
    match (interface, attr_name) {
        // ─── STATE-BACKED: value, checked, indeterminate, selected, selectedIndex ─
        ("HTMLInputElement", "value")
        | ("HTMLTextAreaElement", "value")
        | ("HTMLSelectElement", "value")
        | ("HTMLOptionElement", "value")
        | ("HTMLButtonElement", "value")
        | ("HTMLOutputElement", "value") => AttrCategory::StateBacked {
            state_field: "value".to_string(),
        },

        ("HTMLInputElement", "checked") => AttrCategory::StateBacked {
            state_field: "checked".to_string(),
        },

        ("HTMLInputElement", "indeterminate") => AttrCategory::StateBacked {
            state_field: "indeterminate".to_string(),
        },

        ("HTMLSelectElement", "selectedIndex") => AttrCategory::StateBacked {
            state_field: "selected_index".to_string(),
        },

        ("HTMLOptionElement", "selected") => AttrCategory::StateBacked {
            state_field: "selected".to_string(),
        },

        // ─── COMPUTED: validity, willValidate, validationMessage, form, labels ────
        (_, "willValidate") if is_form_element(interface) => AttrCategory::Computed {
            compute_fn: "compute_will_validate".to_string(),
        },
        (_, "validationMessage") if is_form_element(interface) => AttrCategory::Computed {
            compute_fn: "get_validation_message".to_string(),
        },
        (_, "form") if is_form_element(interface) => AttrCategory::Computed {
            compute_fn: "find_form_owner".to_string(),
        },
        (_, "labels") if is_form_element(interface) => AttrCategory::Computed {
            compute_fn: "find_labels".to_string(),
        },

        // ─── COMPUTED: Select element ─────────────────────────────────────────────
        ("HTMLSelectElement", "type") => AttrCategory::Computed {
            compute_fn: "compute_select_type".to_string(),
        },
        ("HTMLSelectElement", "length") => AttrCategory::Computed {
            compute_fn: "compute_select_length".to_string(),
        },

        // ─── COMPUTED: Textarea ───────────────────────────────────────────────────
        ("HTMLTextAreaElement", "textLength") => AttrCategory::Computed {
            compute_fn: "compute_text_length".to_string(),
        },
        ("HTMLTextAreaElement", "type") => AttrCategory::Computed {
            compute_fn: "compute_textarea_type".to_string(),
        },

        // ─── COMPUTED: HTMLAnchorElement URL decomposition ─────────────────────────
        ("HTMLAnchorElement", "origin") => AttrCategory::Computed {
            compute_fn: "compute_url_origin".to_string(),
        },
        ("HTMLAnchorElement", "protocol") => AttrCategory::Computed {
            compute_fn: "compute_url_protocol".to_string(),
        },
        ("HTMLAnchorElement", "hostname") => AttrCategory::Computed {
            compute_fn: "compute_url_hostname".to_string(),
        },
        ("HTMLAnchorElement", "host") => AttrCategory::Computed {
            compute_fn: "compute_url_host".to_string(),
        },
        ("HTMLAnchorElement", "port") => AttrCategory::Computed {
            compute_fn: "compute_url_port".to_string(),
        },
        ("HTMLAnchorElement", "pathname") => AttrCategory::Computed {
            compute_fn: "compute_url_pathname".to_string(),
        },
        ("HTMLAnchorElement", "search") => AttrCategory::Computed {
            compute_fn: "compute_url_search".to_string(),
        },
        ("HTMLAnchorElement", "hash") => AttrCategory::Computed {
            compute_fn: "compute_url_hash".to_string(),
        },
        ("HTMLAnchorElement", "username") | ("HTMLAnchorElement", "password") => {
            AttrCategory::Computed {
                compute_fn: format!("compute_url_{}", attr_name),
            }
        }

        // ─── COMPUTED: HTMLImageElement ────────────────────────────────────────────
        ("HTMLImageElement", "naturalWidth") | ("HTMLImageElement", "naturalHeight") => {
            AttrCategory::Computed {
                compute_fn: format!("compute_img_{}", to_snake(attr_name)),
            }
        }
        ("HTMLImageElement", "complete") => AttrCategory::Computed {
            compute_fn: "compute_img_complete".to_string(),
        },
        ("HTMLImageElement", "currentSrc") => AttrCategory::Computed {
            compute_fn: "compute_img_current_src".to_string(),
        },

        // ─── COMPUTED: Progress/Meter ─────────────────────────────────────────────
        ("HTMLProgressElement", "position") => AttrCategory::Computed {
            compute_fn: "compute_progress_position".to_string(),
        },

        // ─── COMPUTED: Form element ───────────────────────────────────────────────
        ("HTMLFormElement", "length") => AttrCategory::Computed {
            compute_fn: "compute_form_length".to_string(),
        },

        // ─── COMPUTED: Table row/cell indices ─────────────────────────────────────
        ("HTMLTableRowElement", "rowIndex") | ("HTMLTableRowElement", "sectionRowIndex") => {
            AttrCategory::Computed {
                compute_fn: format!("compute_{}", to_snake(attr_name)),
            }
        }
        ("HTMLTableCellElement", "cellIndex") => AttrCategory::Computed {
            compute_fn: "compute_cell_index".to_string(),
        },

        // ─── COMPUTED: Option index ───────────────────────────────────────────────
        ("HTMLOptionElement", "index") => AttrCategory::Computed {
            compute_fn: "compute_option_index".to_string(),
        },

        // ─── REFLECTED: defaultValue → attr "value", defaultChecked → attr "checked"
        ("HTMLInputElement", "defaultValue") | ("HTMLTextAreaElement", "defaultValue") => {
            AttrCategory::Reflected {
                html_attr: "value".to_string(),
            }
        }
        ("HTMLInputElement", "defaultChecked") => AttrCategory::Reflected {
            html_attr: "checked".to_string(),
        },
        ("HTMLOptionElement", "defaultSelected") => AttrCategory::Reflected {
            html_attr: "selected".to_string(),
        },

        // ─── DEFAULT: Allt annat → Reflected ──────────────────────────────────────
        _ => AttrCategory::Reflected {
            html_attr: to_html_attr(attr_name),
        },
    }
}

/// Klassificera en operation
pub fn classify_operation(interface: &str, op_name: &str) -> OpCategory {
    match (interface, op_name) {
        (_, "checkValidity") if is_form_element(interface) || interface == "HTMLFormElement" => {
            OpCategory::ComputedCall {
                compute_fn: "check_validity".to_string(),
            }
        }
        (_, "reportValidity") if is_form_element(interface) || interface == "HTMLFormElement" => {
            OpCategory::ComputedCall {
                compute_fn: "check_validity".to_string(),
            }
        }
        (_, "setCustomValidity") => OpCategory::StateMutator {
            state_field: "custom_validity".to_string(),
        },
        _ => OpCategory::Stub,
    }
}

fn is_form_element(interface: &str) -> bool {
    matches!(
        interface,
        "HTMLInputElement"
            | "HTMLSelectElement"
            | "HTMLTextAreaElement"
            | "HTMLButtonElement"
            | "HTMLFieldSetElement"
            | "HTMLOutputElement"
    )
}

fn to_html_attr(name: &str) -> String {
    match name {
        "className" => "class".to_string(),
        "htmlFor" => "for".to_string(),
        "readOnly" => "readonly".to_string(),
        "noValidate" | "formNoValidate" => name.to_ascii_lowercase(),
        "httpEquiv" => "http-equiv".to_string(),
        _ => name.to_ascii_lowercase(),
    }
}

fn to_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}
