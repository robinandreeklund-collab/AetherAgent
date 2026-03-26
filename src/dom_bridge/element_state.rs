// ─── Per-element mutable state ─────────────────────────────────────────────
//
// State som INTE reflekteras till HTML-attribut.
// Hanterar input.value (user-redigerad), input.checked (dynamisk), validity, etc.

use std::collections::HashMap;

/// Per-element mutable state — skapas lazily vid första JS-access
#[derive(Debug, Clone, Default)]
pub(super) struct ElementState {
    /// input.value / textarea.value / select.value (efter programmatic set)
    pub value: Option<String>,
    /// input.checked (efter programmatic set, skiljer sig från checked-attribut)
    pub checked: Option<bool>,
    /// input.indeterminate (inget content-attribut)
    pub indeterminate: bool,
    /// select.selectedIndex (efter programmatic set)
    pub selected_index: Option<i32>,
    /// option.selected (efter programmatic set)
    pub selected: Option<bool>,
    /// Custom validity message (från setCustomValidity)
    pub custom_validity: String,
    /// Dirty flags — spec: "dirty value flag" / "dirty checkedness flag"
    pub value_dirty: bool,
    pub checked_dirty: bool,
}

pub(super) type ElementStateStore = HashMap<u64, ElementState>;
