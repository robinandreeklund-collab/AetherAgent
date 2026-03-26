//! Rust code generation for AetherAgent DOM bridge
//!
//! Genererar JsHandler-structs och registreringsfunktioner från parsade WebIDL-interfaces.

use crate::type_map;
use crate::Interface;
use std::fmt::Write;
use std::fs;
use std::path::Path;

/// Generera alla filer
pub fn generate_all(interfaces: &[Interface], output_dir: &str) {
    let mut mod_rs = String::from(
        "// ─── Auto-genererade DOM bindings från WebIDL ──────────────────────────────\n\
         //\n\
         // Genererat av codegen/src/rust_gen.rs — REDIGERA INTE MANUELLT.\n\
         // Kör: cd codegen && cargo run -- ../webidl/ ../src/dom_bridge/generated/\n\n",
    );

    for iface in interfaces {
        let module_name = to_snake_case(&iface.name);
        let code = generate_interface(iface);

        let file_path = format!("{}/{}.rs", output_dir, module_name);
        fs::write(&file_path, &code).expect("Kan inte skriva genererad fil");
        println!("  Genererade: {}", file_path);

        writeln!(mod_rs, "pub(super) mod {};", module_name).unwrap();
    }

    // Skriv mod.rs
    let mod_path = format!("{}/mod.rs", output_dir);
    fs::write(&mod_path, &mod_rs).expect("Kan inte skriva mod.rs");
    println!("  Genererade: {}", mod_path);

    // Generera master-registreringsfunktion
    let register_code = generate_master_register(interfaces);
    let register_path = format!("{}/register.rs", output_dir);
    fs::write(&register_path, &register_code).expect("Kan inte skriva register.rs");
    println!("  Genererade: {}", register_path);

    // Lägg till register-modulen i mod.rs
    let mut mod_content = fs::read_to_string(&mod_path).unwrap();
    mod_content.push_str("\npub(super) mod register;\n");
    fs::write(&mod_path, &mod_content).unwrap();
}

/// Generera kod för ett interface
fn generate_interface(iface: &Interface) -> String {
    let mut code = String::new();

    // Header
    writeln!(
        code,
        "#![allow(unused_imports, dead_code, unused_variables, clippy::all)]\n\
         // Auto-genererat från WebIDL: {}\n\
         // REDIGERA INTE — genereras av codegen.\n\n\
         use rquickjs::{{Ctx, Value}};\n\
         use crate::arena_dom::NodeKey;\n\
         use crate::event_loop::JsHandler;\n\
         use super::super::state::SharedState;\n",
        iface.name
    )
    .unwrap();

    // Generera getter-structs för readonly + read-write attribut
    for attr in &iface.attributes {
        generate_attribute_getter(&mut code, iface, attr);
        if !attr.readonly {
            generate_attribute_setter(&mut code, iface, attr);
        }
    }

    // Generera operation-structs
    for op in &iface.operations {
        generate_operation(&mut code, iface, op);
    }

    // Generera registreringsfunktion
    generate_register_fn(&mut code, iface);

    code
}

/// Generera getter-struct för ett attribut
fn generate_attribute_getter(code: &mut String, iface: &Interface, attr: &crate::Attribute) {
    let struct_name = format!("{}Get{}", iface.name, to_pascal_case(&attr.name));
    let default = type_map::attr_default(&iface.name, &attr.name, &attr.idl_type);
    let attr_key = to_html_attr_name(&attr.name);

    writeln!(code, "pub(crate) struct {} {{", struct_name).unwrap();
    writeln!(code, "    pub(crate) state: SharedState,").unwrap();
    writeln!(code, "    pub(crate) key: NodeKey,").unwrap();
    writeln!(code, "}}").unwrap();

    writeln!(code, "impl JsHandler for {} {{", struct_name).unwrap();
    writeln!(
        code,
        "    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {{"
    )
    .unwrap();
    writeln!(code, "        let s = self.state.borrow();").unwrap();

    let base_type = attr.idl_type.trim_end_matches('?');
    match base_type {
        "boolean" => {
            writeln!(
                code,
                "        let val = s.arena.nodes.get(self.key).map(|n| n.has_attr(\"{}\")).unwrap_or({});",
                attr_key, default
            )
            .unwrap();
            writeln!(code, "        Ok(Value::new_bool(ctx.clone(), val))").unwrap();
        }
        "long" | "unsigned long" => {
            writeln!(
                code,
                "        let val = s.arena.nodes.get(self.key).and_then(|n| n.get_attr(\"{}\")).and_then(|v| v.parse::<i32>().ok()).unwrap_or({});",
                attr_key, default
            )
            .unwrap();
            writeln!(code, "        Ok(Value::new_int(ctx.clone(), val))").unwrap();
        }
        "double" => {
            writeln!(
                code,
                "        let val = s.arena.nodes.get(self.key).and_then(|n| n.get_attr(\"{}\")).and_then(|v| v.parse::<f64>().ok()).unwrap_or({});",
                attr_key, default
            )
            .unwrap();
            writeln!(code, "        Ok(Value::new_float(ctx.clone(), val))").unwrap();
        }
        _ => {
            // DOMString
            writeln!(
                code,
                "        let val = s.arena.nodes.get(self.key).and_then(|n| n.get_attr(\"{}\")).unwrap_or({});",
                attr_key, default
            )
            .unwrap();
            writeln!(
                code,
                "        Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value())"
            )
            .unwrap();
        }
    }

    writeln!(code, "    }}").unwrap();
    writeln!(code, "}}\n").unwrap();
}

/// Generera setter-struct för ett attribut
fn generate_attribute_setter(code: &mut String, iface: &Interface, attr: &crate::Attribute) {
    let struct_name = format!("{}Set{}", iface.name, to_pascal_case(&attr.name));
    let attr_key = to_html_attr_name(&attr.name);

    writeln!(code, "pub(crate) struct {} {{", struct_name).unwrap();
    writeln!(code, "    pub(crate) state: SharedState,").unwrap();
    writeln!(code, "    pub(crate) key: NodeKey,").unwrap();
    writeln!(code, "}}").unwrap();

    writeln!(code, "impl JsHandler for {} {{", struct_name).unwrap();
    writeln!(
        code,
        "    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {{"
    )
    .unwrap();

    let base_type = attr.idl_type.trim_end_matches('?');
    match base_type {
        "boolean" => {
            writeln!(
                code,
                "        let val = args.first().and_then(|v| v.as_bool()).unwrap_or(false);"
            )
            .unwrap();
            writeln!(code, "        let mut s = self.state.borrow_mut();").unwrap();
            writeln!(
                code,
                "        if let Some(n) = s.arena.nodes.get_mut(self.key) {{"
            )
            .unwrap();
            writeln!(
                code,
                "            if val {{ n.set_attr(\"{}\", \"\"); }} else {{ n.remove_attr(\"{}\"); }}",
                attr_key, attr_key
            )
            .unwrap();
            writeln!(code, "        }}").unwrap();
        }
        "long" | "unsigned long" | "double" => {
            writeln!(
                code,
                "        let val = {};",
                type_map::arg_extraction(&attr.idl_type, 0)
            )
            .unwrap();
            writeln!(code, "        let mut s = self.state.borrow_mut();").unwrap();
            writeln!(
                code,
                "        if let Some(n) = s.arena.nodes.get_mut(self.key) {{"
            )
            .unwrap();
            writeln!(
                code,
                "            n.set_attr(\"{}\", &val.to_string());",
                attr_key
            )
            .unwrap();
            writeln!(code, "        }}").unwrap();
        }
        _ => {
            writeln!(
                code,
                "        let val = {};",
                type_map::arg_extraction("DOMString", 0)
            )
            .unwrap();
            writeln!(code, "        let mut s = self.state.borrow_mut();").unwrap();
            writeln!(
                code,
                "        if let Some(n) = s.arena.nodes.get_mut(self.key) {{"
            )
            .unwrap();
            writeln!(code, "            n.set_attr(\"{}\", &val);", attr_key).unwrap();
            writeln!(code, "        }}").unwrap();
        }
    }

    writeln!(code, "        Ok(Value::new_undefined(ctx.clone()))").unwrap();
    writeln!(code, "    }}").unwrap();
    writeln!(code, "}}\n").unwrap();
}

/// Generera operation-struct
fn generate_operation(code: &mut String, iface: &Interface, op: &crate::Operation) {
    let struct_name = format!(
        "{}{}",
        iface.name,
        to_pascal_case(&op.name)
    );

    writeln!(code, "pub(crate) struct {} {{", struct_name).unwrap();
    writeln!(code, "    pub(crate) state: SharedState,").unwrap();
    writeln!(code, "    pub(crate) key: NodeKey,").unwrap();
    writeln!(code, "}}").unwrap();

    writeln!(code, "impl JsHandler for {} {{", struct_name).unwrap();
    writeln!(
        code,
        "    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {{"
    )
    .unwrap();

    // Generera stub-implementationer baserat på returtyp
    match op.return_type.as_str() {
        "void" | "undefined" => {
            writeln!(
                code,
                "        // TODO: Implementera {}.{}()",
                iface.name, op.name
            )
            .unwrap();
            writeln!(code, "        Ok(Value::new_undefined(ctx.clone()))").unwrap();
        }
        "boolean" => {
            match op.name.as_str() {
                "checkValidity" | "reportValidity" => {
                    writeln!(code, "        Ok(Value::new_bool(ctx.clone(), true))").unwrap();
                }
                _ => {
                    writeln!(code, "        Ok(Value::new_bool(ctx.clone(), false))").unwrap();
                }
            }
        }
        _ => {
            writeln!(
                code,
                "        Ok(rquickjs::String::from_str(ctx.clone(), \"\")?.into_value())"
            )
            .unwrap();
        }
    }

    writeln!(code, "    }}").unwrap();
    writeln!(code, "}}\n").unwrap();
}

/// Generera register-funktion som sätter alla properties/metoder på ett objekt
fn generate_register_fn(code: &mut String, iface: &Interface) {
    let fn_name = format!("register_{}", to_snake_case(&iface.name));

    writeln!(code, "/// Registrera alla {}-properties och metoder på ett JS-objekt.", iface.name).unwrap();
    writeln!(code, "pub(crate) fn {}<'js>(", fn_name).unwrap();
    writeln!(code, "    ctx: &Ctx<'js>,").unwrap();
    writeln!(code, "    obj: &rquickjs::Object<'js>,").unwrap();
    writeln!(code, "    state: &SharedState,").unwrap();
    writeln!(code, "    key: NodeKey,").unwrap();
    writeln!(code, ") -> rquickjs::Result<()> {{").unwrap();
    writeln!(code, "    use std::rc::Rc;").unwrap();
    writeln!(
        code,
        "    use rquickjs::{{Function, object::Accessor}};"
    )
    .unwrap();
    writeln!(code, "    use crate::event_loop::JsFn;\n").unwrap();

    for attr in &iface.attributes {
        let getter_name = format!("{}Get{}", iface.name, to_pascal_case(&attr.name));
        let js_name = &attr.name;

        if attr.readonly {
            // Readonly: getter-only accessor
            writeln!(code, "    obj.prop(\"{}\", Accessor::new_get(JsFn({} {{", js_name, getter_name).unwrap();
            writeln!(code, "        state: Rc::clone(state), key,").unwrap();
            writeln!(code, "    }})))?;").unwrap();
        } else {
            // Read-write: getter + setter accessor
            let setter_name = format!("{}Set{}", iface.name, to_pascal_case(&attr.name));
            writeln!(code, "    obj.prop(\"{}\", Accessor::new(", js_name).unwrap();
            writeln!(code, "        JsFn({} {{ state: Rc::clone(state), key }}),", getter_name).unwrap();
            writeln!(code, "        JsFn({} {{ state: Rc::clone(state), key }}),", setter_name).unwrap();
            writeln!(code, "    ))?;").unwrap();
        }
    }

    for op in &iface.operations {
        let struct_name = format!(
            "{}{}",
            iface.name,
            to_pascal_case(&op.name)
        );
        writeln!(
            code,
            "    obj.set(\"{}\", Function::new(ctx.clone(), JsFn({} {{",
            op.name, struct_name
        )
        .unwrap();
        writeln!(code, "        state: Rc::clone(state), key,").unwrap();
        writeln!(code, "    }}))?)?;").unwrap();
    }

    writeln!(code, "    Ok(())").unwrap();
    writeln!(code, "}}").unwrap();
}

/// Generera master-registreringsfunktion som anropas från make_element_object
fn generate_master_register(interfaces: &[Interface]) -> String {
    let mut code = String::new();

    writeln!(
        code,
        "// Auto-genererad master-registreringsfunktion\n\
         // REDIGERA INTE — genereras av codegen.\n\n\
         use rquickjs::{{Ctx, Object}};\n\
         use crate::arena_dom::NodeKey;\n\
         use super::super::state::SharedState;\n"
    )
    .unwrap();

    writeln!(
        code,
        "/// Registrera auto-genererade HTML element properties baserat på tag-namn."
    )
    .unwrap();
    writeln!(
        code,
        "pub(crate) fn register_html_element_properties<'js>("
    )
    .unwrap();
    writeln!(code, "    ctx: &Ctx<'js>,").unwrap();
    writeln!(code, "    obj: &Object<'js>,").unwrap();
    writeln!(code, "    state: &SharedState,").unwrap();
    writeln!(code, "    key: NodeKey,").unwrap();
    writeln!(code, "    tag: &str,").unwrap();
    writeln!(code, ") -> rquickjs::Result<()> {{").unwrap();
    writeln!(code, "    match tag {{").unwrap();

    // Mappa interface-namn till HTML-tag
    let tag_map: Vec<(&str, &str)> = vec![
        ("HTMLInputElement", "input"),
        ("HTMLButtonElement", "button"),
        ("HTMLSelectElement", "select"),
        ("HTMLTextAreaElement", "textarea"),
        ("HTMLFormElement", "form"),
        ("HTMLAnchorElement", "a"),
        ("HTMLImageElement", "img"),
        ("HTMLOptionElement", "option"),
        ("HTMLLabelElement", "label"),
        ("HTMLFieldSetElement", "fieldset"),
        ("HTMLOutputElement", "output"),
        ("HTMLLegendElement", "legend"),
        ("HTMLProgressElement", "progress"),
        ("HTMLMeterElement", "meter"),
    ];

    for (iface_name, tag) in &tag_map {
        if interfaces.iter().any(|i| i.name == *iface_name) {
            let mod_name = to_snake_case(iface_name);
            let fn_name = format!("register_{}", mod_name);
            writeln!(
                code,
                "        \"{}\" => super::{}::{}(ctx, obj, state, key)?,",
                tag, mod_name, fn_name
            )
            .unwrap();
        }
    }

    writeln!(code, "        _ => {{}}").unwrap();
    writeln!(code, "    }}").unwrap();
    writeln!(code, "    Ok(())").unwrap();
    writeln!(code, "}}").unwrap();

    code
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            // Undvik dubbla understreck (HTMLInput → html_input, inte h_t_m_l_input)
            let prev = name.chars().nth(i - 1).unwrap_or('a');
            if !prev.is_uppercase() {
                result.push('_');
            }
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in name.chars() {
        if c == '_' || c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

/// Konvertera IDL-attributnamn till HTML-attributnamn
/// camelCase → kebab-case i de flesta fall, men en del har speciella mappningar
fn to_html_attr_name(idl_name: &str) -> String {
    match idl_name {
        // Speciella mappningar
        "className" => "class".to_string(),
        "htmlFor" => "for".to_string(),
        "defaultChecked" => "checked".to_string(),
        "defaultValue" => "value".to_string(),
        "defaultSelected" => "selected".to_string(),
        "readOnly" => "readonly".to_string(),
        "noValidate" | "formNoValidate" => idl_name.to_ascii_lowercase(),
        // De flesta HTML-attribut är lowercase
        _ => idl_name.to_ascii_lowercase(),
    }
}
