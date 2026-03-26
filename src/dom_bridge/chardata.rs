// CharacterData-metoder: substringData, appendData, insertData, deleteData, replaceData

use rquickjs::{Ctx, Value};

use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;

use super::state::SharedState;
use super::{utf16_len, utf16_offset_to_byte, webidl_unsigned_long};

pub(super) struct SubstringData {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for SubstringData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        if args.len() < 2 {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "TypeError: Not enough arguments")?.into(),
            ));
        }
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let s = self.state.borrow();
        let data = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_deref())
            .unwrap_or("");
        if offset > utf16_len(data) {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        let byte_start = utf16_offset_to_byte(data, offset);
        let byte_end = utf16_offset_to_byte(data, offset + count);
        let result = &data[byte_start..byte_end];
        Ok(rquickjs::String::from_str(ctx.clone(), result)?.into_value())
    }
}

pub(super) struct AppendData {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for AppendData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        if args.is_empty() {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "TypeError: Not enough arguments")?.into(),
            ));
        }
        let data = args[0]
            .as_string()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let mut current = node.text.as_deref().unwrap_or("").to_string();
            current.push_str(&data);
            node.text = Some(current.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct InsertData {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for InsertData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let data = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            let safe_offset = utf16_offset_to_byte(&current, offset);
            let mut new_text = String::with_capacity(current.len() + data.len());
            new_text.push_str(&current[..safe_offset]);
            new_text.push_str(&data);
            new_text.push_str(&current[safe_offset..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct DeleteData {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for DeleteData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            let safe_start = utf16_offset_to_byte(&current, offset);
            let safe_end = utf16_offset_to_byte(&current, offset + count);
            let mut new_text = String::with_capacity(current.len());
            new_text.push_str(&current[..safe_start]);
            new_text.push_str(&current[safe_end..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct ReplaceData {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ReplaceData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let data = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            let safe_start = utf16_offset_to_byte(&current, offset);
            let safe_end = utf16_offset_to_byte(&current, offset + count);
            let mut new_text = String::with_capacity(current.len());
            new_text.push_str(&current[..safe_start]);
            new_text.push_str(&data);
            new_text.push_str(&current[safe_end..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}
