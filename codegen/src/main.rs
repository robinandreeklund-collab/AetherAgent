//! AetherAgent WebIDL → QuickJS Code Generator
//!
//! Läser .webidl-filer och genererar JsHandler-structs + registreringskod
//! för AetherAgents dom_bridge.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

mod rust_gen;
mod type_map;
mod webidl_parser;

fn main() {
    let args: Vec<String> = env::args().collect();

    let webidl_dir = args.get(1).map(String::as_str).unwrap_or("../webidl");
    let output_dir = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("../src/dom_bridge/generated");

    // Samla alla .webidl-filer — försök webidl-rs först, fallback till regex-parser
    let mut interfaces: Vec<Interface> = Vec::new();

    for entry in fs::read_dir(webidl_dir).expect("Kan inte läsa webidl-katalog") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("webidl") {
            let source = fs::read_to_string(&path).expect("Kan inte läsa fil");

            // Försök webidl-rs (fullständig spec-parser) först
            match webidl_parser::parse_webidl_full(&source) {
                Ok(parsed) => {
                    println!(
                        "Parsar (webidl-rs): {} — {} interfaces",
                        path.display(),
                        parsed.len()
                    );
                    interfaces.extend(parsed);
                }
                Err(e) => {
                    // Fallback till regex-parser
                    println!(
                        "Parsar (fallback): {} (webidl-rs: {})",
                        path.display(),
                        e
                    );
                    let parsed = parse_webidl(&source);
                    interfaces.extend(parsed);
                }
            }
        }
    }

    println!("\nParsade {} interfaces:", interfaces.len());
    for iface in &interfaces {
        println!(
            "  {} : {} — {} attrs, {} methods",
            iface.name,
            iface.inherits.as_deref().unwrap_or("(none)"),
            iface.attributes.len(),
            iface.operations.len()
        );
    }

    // Generera Rust-kod
    fs::create_dir_all(output_dir).expect("Kan inte skapa output-katalog");
    rust_gen::generate_all(&interfaces, output_dir);

    println!("\nGenererade filer i {}/", output_dir);
}

// ─── Enkel WebIDL-parser (regex-baserad, inte full spec) ─────────────────────
//
// Vi använder en förenklad parser istället för webidl-rs för PoC.
// Hanterar: interface X : Y { attribute T name; RetType method(args); };

#[derive(Debug, Clone)]
pub struct Interface {
    pub name: String,
    pub inherits: Option<String>,
    pub attributes: Vec<Attribute>,
    pub operations: Vec<Operation>,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub idl_type: String,
    pub readonly: bool,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub name: String,
    pub return_type: String,
    pub arguments: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub name: String,
    pub idl_type: String,
}

fn parse_webidl(source: &str) -> Vec<Interface> {
    let mut interfaces = Vec::new();
    let mut current: Option<Interface> = None;

    for line in source.lines() {
        let trimmed = line.trim();

        // Skippa kommentarer och tomma rader
        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        // interface Name : Parent {
        if trimmed.starts_with("interface ") {
            // Avsluta föregående interface
            if let Some(iface) = current.take() {
                interfaces.push(iface);
            }

            let rest = &trimmed["interface ".len()..];
            let (name, inherits) = if let Some(colon_pos) = rest.find(':') {
                let name = rest[..colon_pos].trim().to_string();
                let parent = rest[colon_pos + 1..]
                    .trim()
                    .trim_end_matches('{')
                    .trim()
                    .to_string();
                (name, Some(parent))
            } else {
                let name = rest
                    .trim_end_matches('{')
                    .trim_end_matches(';')
                    .trim()
                    .to_string();
                (name, None)
            };

            current = Some(Interface {
                name,
                inherits,
                attributes: Vec::new(),
                operations: Vec::new(),
            });
            continue;
        }

        // };
        if trimmed == "};" {
            if let Some(iface) = current.take() {
                interfaces.push(iface);
            }
            continue;
        }

        // Bara processa om vi är inuti en interface
        let iface = match current.as_mut() {
            Some(i) => i,
            None => continue,
        };

        // readonly attribute Type name;
        if trimmed.contains("attribute ") {
            let readonly = trimmed.starts_with("readonly");
            let attr_part = if readonly {
                trimmed
                    .strip_prefix("readonly")
                    .unwrap_or(trimmed)
                    .trim()
                    .strip_prefix("attribute")
                    .unwrap_or(trimmed)
                    .trim()
            } else {
                trimmed
                    .strip_prefix("attribute")
                    .unwrap_or(trimmed)
                    .trim()
            };

            let attr_part = attr_part.trim_end_matches(';').trim();

            // Sista ordet är namnet, resten är typen
            if let Some(last_space) = attr_part.rfind(' ') {
                let idl_type = attr_part[..last_space].trim().to_string();
                let name = attr_part[last_space + 1..].trim().to_string();

                iface.attributes.push(Attribute {
                    name,
                    idl_type,
                    readonly,
                });
            }
            continue;
        }

        // ReturnType methodName(args);
        if trimmed.contains('(') && trimmed.contains(')') && trimmed.ends_with(';') {
            let trimmed = trimmed.trim_end_matches(';').trim();

            if let Some(paren_start) = trimmed.find('(') {
                let before_paren = trimmed[..paren_start].trim();
                let args_str = &trimmed[paren_start + 1..trimmed.len() - 1];

                // before_paren = "RetType methodName" eller "void methodName"
                if let Some(last_space) = before_paren.rfind(' ') {
                    let return_type = before_paren[..last_space].trim().to_string();
                    let name = before_paren[last_space + 1..].trim().to_string();

                    let arguments = if args_str.trim().is_empty() {
                        Vec::new()
                    } else {
                        args_str
                            .split(',')
                            .filter_map(|arg| {
                                let arg = arg.trim();
                                if let Some(last_space) = arg.rfind(' ') {
                                    Some(Argument {
                                        idl_type: arg[..last_space].trim().to_string(),
                                        name: arg[last_space + 1..].trim().to_string(),
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect()
                    };

                    iface.operations.push(Operation {
                        name,
                        return_type,
                        arguments,
                    });
                }
            }
            continue;
        }
    }

    // Sista interface
    if let Some(iface) = current {
        interfaces.push(iface);
    }

    interfaces
}
