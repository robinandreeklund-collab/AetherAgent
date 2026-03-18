use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Parsar HTML-sträng till ett rcdom-träd
pub fn parse_html(html: &str) -> RcDom {
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap_or_else(|_| RcDom::default())
}

/// Rekursiv helper för att hämta all text ur ett DOM-träd
/// Taggar vars textinnehåll ska ignoreras vid text-extraktion
const TEXT_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

pub fn extract_text(handle: &Handle) -> String {
    let mut text = String::new();

    match &handle.data {
        NodeData::Text { contents } => {
            let t = contents.borrow().to_string();
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                text.push_str(trimmed);
                text.push(' ');
            }
        }
        NodeData::Element { name, .. } => {
            // Skippa script/style/noscript — deras text är kod, inte innehåll
            if TEXT_SKIP_TAGS.contains(&name.local.as_ref()) {
                return text;
            }
            for child in handle.children.borrow().iter() {
                text.push_str(&extract_text(child));
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                text.push_str(&extract_text(child));
            }
        }
    }

    text
}

/// Hämta ett specifikt attributvärde från ett element
pub fn get_attr(handle: &Handle, attr_name: &str) -> Option<String> {
    if let NodeData::Element { attrs, .. } = &handle.data {
        for attr in attrs.borrow().iter() {
            if &attr.name.local == attr_name {
                return Some(attr.value.to_string());
            }
        }
    }
    None
}

/// Hämta elementets taggnamn
pub fn get_tag_name(handle: &Handle) -> Option<String> {
    if let NodeData::Element { name, .. } = &handle.data {
        Some(name.local.to_string())
    } else {
        None
    }
}

/// Kontrollera om elementet är synligt (enkel heuristik)
pub fn is_likely_visible(handle: &Handle) -> bool {
    // Kolla style-attribut för display:none / visibility:hidden
    if let Some(style) = get_attr(handle, "style") {
        let normalized: String = style
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if normalized.contains("display:none") || normalized.contains("visibility:hidden") {
            return false;
        }
    }

    // Kolla hidden-attribut
    if let NodeData::Element { attrs, .. } = &handle.data {
        for attr in attrs.borrow().iter() {
            if &attr.name.local == "hidden" {
                return false;
            }
        }
    }

    true
}

/// Inferera semantisk roll från HTML-tagg + ARIA-attribut
pub fn infer_role(handle: &Handle) -> String {
    // ARIA-roll har högst prioritet
    if let Some(role) = get_attr(handle, "role") {
        if !role.is_empty() {
            return role;
        }
    }

    let tag = get_tag_name(handle).unwrap_or_default();
    let input_type = get_attr(handle, "type").unwrap_or_default().to_lowercase();

    match tag.as_str() {
        "button" => "button".to_string(),
        "a" => "link".to_string(),
        "input" => match input_type.as_str() {
            "checkbox" => "checkbox".to_string(),
            "radio" => "radio".to_string(),
            "submit" | "button" | "reset" => "button".to_string(),
            "search" => "searchbox".to_string(),
            _ => "textbox".to_string(),
        },
        "textarea" => "textarea".to_string(),
        "select" => "combobox".to_string(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading".to_string(),
        "img" => "img".to_string(),
        "nav" => "navigation".to_string(),
        "main" => "main".to_string(),
        "header" => "banner".to_string(),
        "footer" => "contentinfo".to_string(),
        "form" => "form".to_string(),
        "table" => "table".to_string(),
        "li" => "listitem".to_string(),
        "ul" | "ol" => "list".to_string(),
        "p" | "span" | "div" => "text".to_string(),
        _ => "generic".to_string(),
    }
}

/// Extrahera label för ett element (WCAG-fallback-kedja)
pub fn extract_label(handle: &Handle) -> String {
    // 1. aria-label
    if let Some(label) = get_attr(handle, "aria-label") {
        if !label.trim().is_empty() {
            return label.trim().to_string();
        }
    }

    // 2. aria-labelledby (vi hämtar bara id:t, fullständig resolving kräver hela DOM-kontext)
    if let Some(labelledby) = get_attr(handle, "aria-labelledby") {
        if !labelledby.trim().is_empty() {
            return format!("[ref:{}]", labelledby.trim());
        }
    }

    // 3. placeholder för inputs
    if let Some(placeholder) = get_attr(handle, "placeholder") {
        if !placeholder.trim().is_empty() {
            return placeholder.trim().to_string();
        }
    }

    // 4. alt-text för bilder
    if let Some(alt) = get_attr(handle, "alt") {
        if !alt.trim().is_empty() {
            return alt.trim().to_string();
        }
    }

    // 5. title-attribut
    if let Some(title) = get_attr(handle, "title") {
        if !title.trim().is_empty() {
            return title.trim().to_string();
        }
    }

    // 6. Inre text (WCAG-fallback)
    let inner = extract_text(handle);
    let trimmed = inner.trim();
    if !trimmed.is_empty() {
        // Begränsa till 80 tecken
        let truncated: String = trimmed.chars().take(80).collect();
        return truncated;
    }

    // 7. name-attribut som sista utväg
    if let Some(name) = get_attr(handle, "name") {
        if !name.trim().is_empty() {
            return name.trim().to_string();
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_html() {
        let html = r#"<html><body><button>Klicka här</button></body></html>"#;
        let dom = parse_html(html);
        assert!(!dom.document.children.borrow().is_empty());
    }

    #[test]
    fn test_aria_label_priority() {
        let html = r#"<html><body><button aria-label="Stäng dialog">X</button></body></html>"#;
        let dom = parse_html(html);

        // Hitta button-elementet via rekursiv sökning
        fn find_button(handle: &Handle) -> Option<Handle> {
            if let Some(tag) = get_tag_name(handle) {
                if tag == "button" {
                    return Some(handle.clone());
                }
            }
            for child in handle.children.borrow().iter() {
                if let Some(found) = find_button(child) {
                    return Some(found);
                }
            }
            None
        }

        let button = find_button(&dom.document).expect("Borde hitta button");
        let label = extract_label(&button);
        assert_eq!(
            label, "Stäng dialog",
            "aria-label ska ha prioritet över inner text"
        );
    }
}
