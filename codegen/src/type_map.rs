//! WebIDL Type → Rust code mapping

/// Returnerar Rust-kod för att extrahera ett argument av given WebIDL-typ
pub fn arg_extraction(idl_type: &str, index: usize) -> String {
    let base = idl_type.trim_end_matches('?');
    match base {
        "DOMString" => format!(
            "args.get({}).and_then(|v| v.as_string()).and_then(|s| s.to_string().ok()).unwrap_or_default()",
            index
        ),
        "boolean" => format!(
            "args.get({}).and_then(|v| v.as_bool()).unwrap_or(false)",
            index
        ),
        "long" => format!(
            "args.get({}).and_then(|v| v.as_int()).unwrap_or(0) as i32",
            index
        ),
        "unsigned long" => format!(
            "args.get({}).and_then(|v| v.as_int()).unwrap_or(0) as u32",
            index
        ),
        "double" => format!(
            "args.get({}).and_then(|v| v.as_float()).or_else(|| args.get({}).and_then(|v| v.as_int().map(|i| i as f64))).unwrap_or(0.0)",
            index, index
        ),
        _ => format!(
            "args.get({}).and_then(|v| v.as_string()).and_then(|s| s.to_string().ok()).unwrap_or_default()",
            index
        ),
    }
}

/// Returnerar Rust-kod för att returnera ett värde av given WebIDL-typ
pub fn return_value(idl_type: &str, var_name: &str) -> String {
    let nullable = idl_type.ends_with('?');
    let base = idl_type.trim_end_matches('?');

    match base {
        "void" | "undefined" => "Ok(Value::new_undefined(ctx.clone()))".to_string(),
        "boolean" => format!("Ok(Value::new_bool(ctx.clone(), {}))", var_name),
        "DOMString" => {
            if nullable {
                format!(
                    "match {} {{ Some(ref s) => Ok(rquickjs::String::from_str(ctx.clone(), s)?.into_value()), None => Ok(Value::new_null(ctx.clone())) }}",
                    var_name
                )
            } else {
                format!(
                    "Ok(rquickjs::String::from_str(ctx.clone(), &{})?.into_value())",
                    var_name
                )
            }
        }
        "long" | "unsigned long" => format!("Ok(Value::new_int(ctx.clone(), {} as i32))", var_name),
        "double" => format!("Ok(Value::new_float(ctx.clone(), {}))", var_name),
        _ => format!(
            "Ok(rquickjs::String::from_str(ctx.clone(), &{})?.into_value())",
            var_name
        ),
    }
}

/// Returnerar Rust-typen för en WebIDL-typ
pub fn rust_type(idl_type: &str) -> &str {
    let base = idl_type.trim_end_matches('?');
    match base {
        "DOMString" => "String",
        "boolean" => "bool",
        "long" => "i32",
        "unsigned long" => "u32",
        "double" => "f64",
        _ => "String",
    }
}

/// Returnerar default-värde för en WebIDL-typ
pub fn default_value(idl_type: &str) -> &str {
    let base = idl_type.trim_end_matches('?');
    match base {
        "DOMString" => "\"\"",
        "boolean" => "false",
        "long" | "unsigned long" => "0",
        "double" => "0.0",
        _ => "\"\"",
    }
}

/// Returnerar default-värde specifikt per attribut+element
pub fn attr_default(interface_name: &str, attr_name: &str, idl_type: &str) -> String {
    // Spec-specifika defaults
    match (interface_name, attr_name) {
        ("HTMLInputElement", "type") => "\"text\"".to_string(),
        ("HTMLButtonElement", "type") => "\"submit\"".to_string(),
        ("HTMLSelectElement", "type") => "\"select-one\"".to_string(),
        ("HTMLTextAreaElement", "type") => "\"textarea\"".to_string(),
        ("HTMLFieldSetElement", "type") => "\"fieldset\"".to_string(),
        ("HTMLOutputElement", "type") => "\"output\"".to_string(),
        ("HTMLInputElement", "size") => "20".to_string(),
        ("HTMLTextAreaElement", "cols") => "20".to_string(),
        ("HTMLTextAreaElement", "rows") => "2".to_string(),
        ("HTMLMeterElement", "min") | ("HTMLProgressElement", "value") => "0.0".to_string(),
        ("HTMLMeterElement", "max") | ("HTMLProgressElement", "max") => "1.0".to_string(),
        ("HTMLProgressElement", "position") => "-1.0".to_string(),
        ("HTMLImageElement", "complete") => "false".to_string(),
        ("HTMLSelectElement", "selectedIndex") => "-1".to_string(),
        _ => default_value(idl_type).to_string(),
    }
}
