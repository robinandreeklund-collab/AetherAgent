//! WebIDL parsing via webidl-rs (0.9) crate
//!
//! Använder lalrpop-baserad fullständig WebIDL-parser.

use crate::{Argument, Attribute, Interface, Operation};
use webidl::ast;

/// Parsa WebIDL-källa med webidl-rs och konvertera till våra Interface-strukturer.
pub fn parse_webidl_full(source: &str) -> Result<Vec<Interface>, String> {
    let definitions =
        webidl::parse_string(source).map_err(|e| format!("WebIDL parse error: {:?}", e))?;

    let mut interfaces = Vec::new();

    for def in &definitions {
        match def {
            ast::Definition::Interface(iface) => match iface {
                ast::Interface::NonPartial(np) => {
                    interfaces.push(convert_non_partial(np));
                }
                ast::Interface::Partial(p) => {
                    interfaces.push(convert_partial(p));
                }
                ast::Interface::Callback(cb) => {
                    interfaces.push(convert_callback_interface(cb));
                }
            },
            ast::Definition::Mixin(mixin) => match mixin {
                ast::Mixin::NonPartial(np) => {
                    let i = convert_mixin(np);
                    if !i.attributes.is_empty() || !i.operations.is_empty() {
                        interfaces.push(i);
                    }
                }
                ast::Mixin::Partial(p) => {
                    let i = convert_partial_mixin(p);
                    if !i.attributes.is_empty() || !i.operations.is_empty() {
                        interfaces.push(i);
                    }
                }
            },
            // Dictionary, enum, typedef, callback, etc. — ignoreras
            _ => {}
        }
    }

    Ok(interfaces)
}

fn convert_non_partial(np: &ast::NonPartialInterface) -> Interface {
    let (attrs, ops) = extract_interface_members(&np.members);
    Interface {
        name: np.name.clone(),
        inherits: np.inherits.clone(),
        attributes: attrs,
        operations: ops,
    }
}

fn convert_partial(p: &ast::PartialInterface) -> Interface {
    let (attrs, ops) = extract_interface_members(&p.members);
    Interface {
        name: p.name.clone(),
        inherits: None,
        attributes: attrs,
        operations: ops,
    }
}

fn convert_callback_interface(cb: &ast::CallbackInterface) -> Interface {
    let (attrs, ops) = extract_interface_members(&cb.members);
    Interface {
        name: cb.name.clone(),
        inherits: cb.inherits.clone(),
        attributes: attrs,
        operations: ops,
    }
}

fn convert_mixin(np: &ast::NonPartialMixin) -> Interface {
    let (attrs, ops) = extract_mixin_members(&np.members);
    Interface {
        name: np.name.clone(),
        inherits: None,
        attributes: attrs,
        operations: ops,
    }
}

fn convert_partial_mixin(p: &ast::PartialMixin) -> Interface {
    let (attrs, ops) = extract_mixin_members(&p.members);
    Interface {
        name: p.name.clone(),
        inherits: None,
        attributes: attrs,
        operations: ops,
    }
}

fn extract_interface_members(members: &[ast::InterfaceMember]) -> (Vec<Attribute>, Vec<Operation>) {
    let mut attrs = Vec::new();
    let mut ops = Vec::new();
    for m in members {
        match m {
            ast::InterfaceMember::Attribute(a) => {
                if let Some(attr) = convert_attribute(a) {
                    attrs.push(attr);
                }
            }
            ast::InterfaceMember::Operation(o) => {
                if let Some(op) = convert_operation(o) {
                    ops.push(op);
                }
            }
            _ => {}
        }
    }
    (attrs, ops)
}

fn extract_mixin_members(members: &[ast::MixinMember]) -> (Vec<Attribute>, Vec<Operation>) {
    let mut attrs = Vec::new();
    let mut ops = Vec::new();
    for m in members {
        match m {
            ast::MixinMember::Attribute(a) => {
                if let Some(attr) = convert_attribute(a) {
                    attrs.push(attr);
                }
            }
            ast::MixinMember::Operation(o) => {
                // MixinMember::Operation innehåller en ast::Operation (enum), inte RegularOperation
                if let Some(op) = convert_operation(o) {
                    ops.push(op);
                }
            }
            _ => {}
        }
    }
    (attrs, ops)
}

fn convert_attribute(attr: &ast::Attribute) -> Option<Attribute> {
    match attr {
        ast::Attribute::Regular(r) => Some(Attribute {
            name: r.name.clone(),
            idl_type: type_to_string(&r.type_),
            readonly: r.read_only,
        }),
        ast::Attribute::Static(s) => Some(Attribute {
            name: s.name.clone(),
            idl_type: type_to_string(&s.type_),
            readonly: s.read_only,
        }),
        ast::Attribute::Stringifier(s) => Some(Attribute {
            name: s.name.clone(),
            idl_type: type_to_string(&s.type_),
            readonly: s.read_only,
        }),
    }
}

fn convert_operation(op: &ast::Operation) -> Option<Operation> {
    match op {
        ast::Operation::Regular(r) => convert_regular_operation(r),
        ast::Operation::Static(s) => {
            let name = s.name.as_ref()?.clone();
            Some(Operation {
                name,
                return_type: return_type_str(&s.return_type),
                arguments: s.arguments.iter().map(convert_arg).collect(),
            })
        }
        ast::Operation::Special(s) => {
            let name = s.name.as_ref()?.clone();
            Some(Operation {
                name,
                return_type: return_type_str(&s.return_type),
                arguments: s.arguments.iter().map(convert_arg).collect(),
            })
        }
        ast::Operation::Stringifier(s) => match s {
            ast::StringifierOperation::Explicit(e) => {
                let name = e.name.as_ref()?.clone();
                Some(Operation {
                    name,
                    return_type: return_type_str(&e.return_type),
                    arguments: e.arguments.iter().map(convert_arg).collect(),
                })
            }
            ast::StringifierOperation::Implicit(_) => None,
        },
    }
}

fn convert_regular_operation(r: &ast::RegularOperation) -> Option<Operation> {
    let name = r.name.as_ref()?.clone();
    Some(Operation {
        name,
        return_type: return_type_str(&r.return_type),
        arguments: r.arguments.iter().map(convert_arg).collect(),
    })
}

fn convert_arg(arg: &ast::Argument) -> Argument {
    Argument {
        name: arg.name.clone(),
        idl_type: type_to_string(&arg.type_),
    }
}

fn return_type_str(rt: &ast::ReturnType) -> String {
    match rt {
        ast::ReturnType::NonVoid(t) => type_to_string(t),
        ast::ReturnType::Void => "void".to_string(),
    }
}

fn type_to_string(ty: &ast::Type) -> String {
    let base = type_kind_str(&ty.kind);
    if ty.nullable {
        format!("{}?", base)
    } else {
        base
    }
}

fn type_kind_str(kind: &ast::TypeKind) -> String {
    match kind {
        ast::TypeKind::Any => "any".to_string(),
        ast::TypeKind::Boolean => "boolean".to_string(),
        ast::TypeKind::Byte => "byte".to_string(),
        ast::TypeKind::Octet => "octet".to_string(),
        ast::TypeKind::SignedShort => "short".to_string(),
        ast::TypeKind::SignedLong => "long".to_string(),
        ast::TypeKind::SignedLongLong => "long long".to_string(),
        ast::TypeKind::UnsignedShort => "unsigned short".to_string(),
        ast::TypeKind::UnsignedLong => "unsigned long".to_string(),
        ast::TypeKind::UnsignedLongLong => "unsigned long long".to_string(),
        ast::TypeKind::RestrictedFloat => "float".to_string(),
        ast::TypeKind::UnrestrictedFloat => "unrestricted float".to_string(),
        ast::TypeKind::RestrictedDouble => "double".to_string(),
        ast::TypeKind::UnrestrictedDouble => "unrestricted double".to_string(),
        ast::TypeKind::DOMString => "DOMString".to_string(),
        ast::TypeKind::ByteString => "ByteString".to_string(),
        ast::TypeKind::USVString => "USVString".to_string(),
        ast::TypeKind::Identifier(id) => id.clone(),
        ast::TypeKind::Object => "object".to_string(),
        ast::TypeKind::Symbol => "symbol".to_string(),
        ast::TypeKind::Error => "Error".to_string(),
        ast::TypeKind::ArrayBuffer
        | ast::TypeKind::DataView
        | ast::TypeKind::Float32Array
        | ast::TypeKind::Float64Array
        | ast::TypeKind::Int8Array
        | ast::TypeKind::Int16Array
        | ast::TypeKind::Int32Array
        | ast::TypeKind::Uint8Array
        | ast::TypeKind::Uint16Array
        | ast::TypeKind::Uint32Array
        | ast::TypeKind::Uint8ClampedArray => "ArrayBuffer".to_string(),
        ast::TypeKind::FrozenArray(inner) => format!("FrozenArray<{}>", type_to_string(inner)),
        ast::TypeKind::Sequence(inner) => format!("sequence<{}>", type_to_string(inner)),
        ast::TypeKind::Promise(inner) => format!("Promise<{}>", return_type_str(inner)),
        ast::TypeKind::Record(_, value) => format!("record<DOMString, {}>", type_to_string(value)),
        ast::TypeKind::Union(types) => {
            let parts: Vec<String> = types.iter().map(|t| type_to_string(t)).collect();
            format!("({})", parts.join(" or "))
        }
    }
}
