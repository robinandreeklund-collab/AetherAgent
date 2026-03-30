use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
#[cfg(feature = "js-eval")]
use html5ever::{parse_fragment, QualName};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Parsar HTML-sträng till ett rcdom-träd
pub fn parse_html(html: &str) -> RcDom {
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap_or_else(|_| RcDom::default())
}

/// Parsar HTML-fragment (för innerHTML) — returnerar en lista av child-noder utan <html>/<head>/<body>-wrapper
#[cfg(feature = "js-eval")]
pub fn parse_html_fragment(html: &str, context_tag: &str) -> RcDom {
    let context = QualName::new(
        None,
        html5ever::ns!(html),
        html5ever::LocalName::from(context_tag),
    );
    parse_fragment(RcDom::default(), Default::default(), context, vec![], false)
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap_or_else(|_| RcDom::default())
}

/// Rekursiv helper för att hämta all text ur ett DOM-träd
/// Taggar vars textinnehåll ska ignoreras vid text-extraktion
const TEXT_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

pub fn extract_text(handle: &Handle) -> String {
    let mut buf = String::new();
    extract_text_into(handle, &mut buf);
    buf
}

/// Samla text i en delad buffer — undviker String-allokering per rekursiv nivå
fn extract_text_into(handle: &Handle, buf: &mut String) {
    match &handle.data {
        NodeData::Text { contents } => {
            let t = contents.borrow();
            let trimmed = t.trim();
            if !trimmed.is_empty() {
                buf.push_str(trimmed);
                buf.push(' ');
            }
        }
        NodeData::Element { name, .. } => {
            if TEXT_SKIP_TAGS.contains(&name.local.as_ref()) {
                return;
            }
            for child in handle.children.borrow().iter() {
                extract_text_into(child, buf);
            }
        }
        _ => {
            for child in handle.children.borrow().iter() {
                extract_text_into(child, buf);
            }
        }
    }
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

/// Cachad batch-extraktion av alla relevanta attribut i en enda pass.
/// Eliminerar 15-20 separata `get_attr()` O(n)-sökningar per element.
pub struct AttrCache {
    pub tag: String,
    pub role: Option<String>,
    pub attr_type: Option<String>,
    pub style: Option<String>,
    pub class: Option<String>,
    pub id: Option<String>,
    pub itemtype: Option<String>,
    pub itemprop: Option<String>,
    pub name: Option<String>,
    pub aria_label: Option<String>,
    pub aria_labelledby: Option<String>,
    pub placeholder: Option<String>,
    pub alt: Option<String>,
    pub title: Option<String>,
    pub src: Option<String>,
    pub has_hidden: bool,
    pub is_aria_hidden: bool,
    pub has_data_product_id: bool,
    pub has_data_product: bool,
    pub has_data_item_id: bool,
    // Lazy-load bild-attribut
    pub lazy_src: Option<String>,
}

impl AttrCache {
    /// Extrahera alla relevanta attribut i en enda pass genom attributlistan
    pub fn from_handle(handle: &Handle) -> Self {
        let mut cache = AttrCache {
            tag: String::new(),
            role: None,
            attr_type: None,
            style: None,
            class: None,
            id: None,
            itemtype: None,
            itemprop: None,
            name: None,
            aria_label: None,
            aria_labelledby: None,
            placeholder: None,
            alt: None,
            title: None,
            src: None,
            has_hidden: false,
            is_aria_hidden: false,
            has_data_product_id: false,
            has_data_product: false,
            has_data_item_id: false,
            lazy_src: None,
        };

        if let NodeData::Element { name, attrs, .. } = &handle.data {
            cache.tag = name.local.to_string();
            for attr in attrs.borrow().iter() {
                let attr_name = &*attr.name.local;
                let val = || attr.value.to_string();
                match attr_name {
                    "role" => cache.role = Some(val()),
                    "type" => cache.attr_type = Some(val()),
                    "style" => cache.style = Some(val()),
                    "class" => cache.class = Some(val()),
                    "id" => cache.id = Some(val()),
                    "itemtype" => cache.itemtype = Some(val()),
                    "itemprop" => cache.itemprop = Some(val()),
                    "name" => cache.name = Some(val()),
                    "aria-label" => cache.aria_label = Some(val()),
                    "aria-labelledby" => cache.aria_labelledby = Some(val()),
                    "placeholder" => cache.placeholder = Some(val()),
                    "alt" => cache.alt = Some(val()),
                    "title" => cache.title = Some(val()),
                    "src" => cache.src = Some(val()),
                    "hidden" => cache.has_hidden = true,
                    "aria-hidden" => {
                        cache.is_aria_hidden = attr.value.trim().eq_ignore_ascii_case("true");
                    }
                    "data-product-id" => cache.has_data_product_id = true,
                    "data-product" => cache.has_data_product = true,
                    "data-item-id" => cache.has_data_item_id = true,
                    "data-src" | "data-lazy-src" | "data-original" | "data-image"
                    | "data-thumb" | "data-thumbnail" => {
                        if cache.lazy_src.is_none() {
                            let v = val();
                            let trimmed = v.trim();
                            if !trimmed.is_empty() {
                                cache.lazy_src = Some(trimmed.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        cache
    }
}

/// ID/klass-mönster som indikerar "JavaScript krävs"-meddelanden
const NOJS_IDENTIFIERS: &[&str] = &["nojs", "no-js", "noscript-warning", "js-disabled"];

/// Kontrollera om elementet är synligt (utökad heuristik)
///
/// Detekterar: display:none, visibility:hidden, opacity:0, aria-hidden="true",
/// HTML hidden-attribut, off-screen positioning (left:-9999px),
/// nojs-divvar (id="nojs", class="no-js" etc.).
pub fn is_likely_visible(handle: &Handle) -> bool {
    let cache = AttrCache::from_handle(handle);
    is_likely_visible_cached(&cache)
}

/// Snabb synlighetscheck med förextraherade attribut (noll extra get_attr-anrop)
#[inline]
pub fn is_likely_visible_cached(cache: &AttrCache) -> bool {
    // Kolla style-attribut för osynlighet
    if let Some(ref style) = cache.style {
        if is_style_hidden(style) {
            return false;
        }
    }

    // HTML5 hidden-attribut
    if cache.has_hidden {
        return false;
    }
    // aria-hidden="true"
    if cache.is_aria_hidden {
        return false;
    }

    // nojs-divvar: id="nojs" eller class="no-js"
    if let Some(ref id) = cache.id {
        if NOJS_IDENTIFIERS
            .iter()
            .any(|pat| id.eq_ignore_ascii_case(pat))
        {
            return false;
        }
    }
    if let Some(ref class) = cache.class {
        if NOJS_IDENTIFIERS.iter().any(|pat| {
            class
                .split_whitespace()
                .any(|c| c.eq_ignore_ascii_case(pat))
        }) {
            return false;
        }
    }

    true
}

/// Avgör om en inline style-sträng döljer elementet
///
/// Zero-allocation: jämför direkt på bytes med ASCII case-insensitive matching,
/// hoppar över whitespace utan att bygga ny sträng.
fn is_style_hidden(style: &str) -> bool {
    // Bygg normaliserad vy utan allokering: skippa whitespace, ASCII-lowercase
    let norm_bytes: Vec<u8> = style
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .map(|b| b.to_ascii_lowercase())
        .collect();
    let norm = norm_bytes.as_slice();

    contains_bytes(norm, b"display:none")
        || contains_bytes(norm, b"visibility:hidden")
        || contains_bytes(norm, b"opacity:0")
        || contains_bytes(norm, b"left:-9999")
        || contains_bytes(norm, b"left:-10000")
        || contains_bytes(norm, b"clip:rect(0")
}

/// Snabb byte-slice contains (undviker UTF-8 overhead)
#[inline]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// CTA-nyckelord som indikerar call-to-action-knappar
const CTA_KEYWORDS: &[&str] = &[
    "add to cart",
    "buy now",
    "purchase",
    "checkout",
    "sign up",
    "subscribe",
    "get started",
    "download",
    "try free",
    "order now",
    "köp",
    "lägg i varukorg",
    "handla",
    "beställ",
    "registrera",
    "prenumerera",
    "ladda ner",
    "kom igång",
];

/// Valutasymboler och nyckelord som indikerar pristext (uppercase för snabb matchning)
const PRICE_INDICATORS: &[&str] = &[
    "$", "€", "£", "¥", "₹", "KR", "SEK", "NOK", "DKK", "USD", "EUR", "GBP",
];

/// Inferera semantisk roll från HTML-tagg + ARIA-attribut + heuristik
pub fn infer_role(handle: &Handle) -> String {
    let cache = AttrCache::from_handle(handle);
    infer_role_cached(handle, &cache)
}

/// Rolldetektering med pre-extraherad text (undviker dubbla extract_text-anrop)
///
/// Om `precomputed_text` anges används den istället för att anropa extract_text.
pub fn infer_role_with_text(cache: &AttrCache, precomputed_text: &str) -> String {
    // ARIA-roll har högst prioritet
    if let Some(ref role) = cache.role {
        if !role.is_empty() {
            return role.clone();
        }
    }

    let input_type_lower: String;
    let input_type_str = if let Some(ref t) = cache.attr_type {
        input_type_lower = t.to_ascii_lowercase();
        input_type_lower.as_str()
    } else {
        ""
    };

    // Tagg-baserad roll (grundläggande)
    let base_role = match cache.tag.as_str() {
        "button" => "button",
        "a" => "link",
        "input" => match input_type_str {
            "checkbox" => "checkbox",
            "radio" => "radio",
            "submit" | "button" | "reset" => "button",
            "search" => "searchbox",
            _ => "textbox",
        },
        "textarea" => "textarea",
        "select" => "combobox",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "img" => "img",
        "nav" => "navigation",
        "main" => "main",
        "header" => "banner",
        "footer" => "contentinfo",
        "form" => "form",
        "table" => "table",
        "li" => "listitem",
        "ul" | "ol" => "list",
        "p" | "span" | "div" => "text",
        _ => "generic",
    };

    // Heuristik: Schema.org product_card
    if let Some(ref itemtype) = cache.itemtype {
        let it_lower_buf: String;
        let it_lower = if itemtype.is_ascii() {
            it_lower_buf = itemtype.to_ascii_lowercase();
            it_lower_buf.as_str()
        } else {
            it_lower_buf = itemtype.to_lowercase();
            it_lower_buf.as_str()
        };
        if it_lower.contains("schema.org/product") || it_lower.contains("schema.org/offer") {
            return "product_card".to_string();
        }
    }
    if cache.has_data_product_id || cache.has_data_product || cache.has_data_item_id {
        if let Some(ref class) = cache.class {
            let cl = class.to_ascii_lowercase();
            if cl.contains("product") || cl.contains("card") || cl.contains("item") {
                return "product_card".to_string();
            }
        }
    }

    // Heuristik: CTA — bara för klickbara element (button/a)
    let class_lower = if base_role == "button"
        || base_role == "link"
        || matches!(base_role, "text" | "generic")
    {
        cache
            .class
            .as_deref()
            .map(|c| c.to_ascii_lowercase())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if base_role == "button" || base_role == "link" {
        let text_lower = precomputed_text.to_ascii_lowercase();
        for kw in CTA_KEYWORDS {
            if text_lower.contains(kw) {
                return "cta".to_string();
            }
        }
        if class_lower.contains("cta")
            || class_lower.contains("add-to-cart")
            || class_lower.contains("buy-btn")
            || class_lower.contains("checkout")
        {
            return "cta".to_string();
        }
    }

    // Heuristik: pristext — spans/divs med valutatecken + siffror
    if matches!(base_role, "text" | "generic")
        && looks_like_price_from_text(precomputed_text, &class_lower, cache)
    {
        return "price".to_string();
    }

    base_role.to_string()
}

/// Snabb rolldetektering med förextraherade attribut (legacy wrapper)
pub fn infer_role_cached(handle: &Handle, cache: &AttrCache) -> String {
    let text = extract_text(handle);
    infer_role_with_text(cache, &text)
}

/// Kontrollera om pre-extraherad text ser ut som ett pris
fn looks_like_price_from_text(text: &str, class_lower: &str, cache: &AttrCache) -> bool {
    let trimmed = text.trim();
    // Tomt eller för långt → inte pris
    if trimmed.is_empty() || trimmed.len() > 40 {
        return false;
    }
    // Måste innehålla minst en siffra
    if !trimmed.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // Kontrollera valutaindikatorer i texten (case-insensitive)
    // PRICE_INDICATORS är redan uppercase — undvik allokering per iteration
    let upper = trimmed.to_uppercase();
    for indicator in PRICE_INDICATORS {
        if upper.contains(indicator) {
            return true;
        }
    }
    // CSS-klass som antyder pris (använd pre-fetched class)
    if class_lower.contains("price") || class_lower.contains("pris") || class_lower.contains("cost")
    {
        return true;
    }
    // itemprop=price — använd cachad version
    if let Some(ref itemprop) = cache.itemprop {
        if itemprop.eq_ignore_ascii_case("price") || itemprop.to_ascii_lowercase().contains("price")
        {
            return true;
        }
    }
    false
}

/// Lazy-load attribut som kan innehålla den riktiga bild-URL:en
#[cfg(test)]
const LAZY_SRC_ATTRS: &[&str] = &[
    "data-src",
    "data-lazy-src",
    "data-original",
    "data-image",
    "data-thumb",
    "data-thumbnail",
];

/// Hämta den effektiva bild-URL:en för ett <img>-element.
///
/// Om `src` saknas eller är en placeholder (data: URI), letar vi i
/// lazy-load-attribut (data-src, data-lazy-src, etc.).
#[cfg(test)]
/// Returnerar `None` om ingen bild-URL hittas.
pub fn resolve_lazy_src(handle: &Handle) -> Option<String> {
    let tag = get_tag_name(handle)?;
    if tag != "img" {
        return None;
    }

    // Kolla om src redan är riktig (inte placeholder)
    if let Some(src) = get_attr(handle, "src") {
        let trimmed = src.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("data:") {
            return Some(trimmed.to_string());
        }
    }

    // Fallback: sök i lazy-load-attribut
    for attr_name in LAZY_SRC_ATTRS {
        if let Some(val) = get_attr(handle, attr_name) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

/// Extrahera label för ett element (WCAG-fallback-kedja)
pub fn extract_label(handle: &Handle) -> String {
    let cache = AttrCache::from_handle(handle);
    extract_label_cached(handle, &cache)
}

/// Snabb label-extraktion med förextraherade attribut
pub fn extract_label_cached(handle: &Handle, cache: &AttrCache) -> String {
    extract_label_with_text(cache, &extract_text(handle))
}

/// Label-extraktion med pre-extraherad text (undviker dubbla extract_text-anrop)
pub fn extract_label_with_text(cache: &AttrCache, inner_text: &str) -> String {
    // 1. aria-label
    if let Some(ref label) = cache.aria_label {
        let trimmed = label.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 2. aria-labelledby
    if let Some(ref labelledby) = cache.aria_labelledby {
        let trimmed = labelledby.trim();
        if !trimmed.is_empty() {
            return format!("[ref:{}]", trimmed);
        }
    }

    // 3. placeholder för inputs
    if let Some(ref placeholder) = cache.placeholder {
        let trimmed = placeholder.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 4. alt-text för bilder
    if let Some(ref alt) = cache.alt {
        let trimmed = alt.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 5. title-attribut
    if let Some(ref title) = cache.title {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 6. Inre text (WCAG-fallback) — använd pre-extraherad text
    let inner = inner_text;
    let trimmed = inner.trim();
    if !trimmed.is_empty() {
        // Begränsa till 300 tecken — tillräckligt för embedding-scoring
        // (80 tecken klippte bort fakta-siffror som satt efter position 80)
        let truncated: String = trimmed.chars().take(300).collect();
        return truncated;
    }

    // 7. name-attribut som sista utväg
    if let Some(ref name) = cache.name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 8. Lazy-loaded bild-URL — använd cachad lazy_src
    let effective_src = cache.lazy_src.as_deref().or_else(|| {
        cache.src.as_deref().and_then(|s| {
            let t = s.trim();
            if !t.is_empty() && !t.starts_with("data:") {
                Some(t)
            } else {
                None
            }
        })
    });
    if cache.tag == "img" {
        if let Some(src) = effective_src {
            if let Some(filename) = src.rsplit('/').next() {
                let clean = filename.split('?').next().unwrap_or(filename);
                if !clean.is_empty() {
                    return format!("[img:{}]", clean);
                }
            }
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Hjälpfunktioner ===

    /// Rekursiv sökning efter element med specifik tagg
    fn find_element(handle: &Handle, target_tag: &str) -> Option<Handle> {
        if let Some(tag) = get_tag_name(handle) {
            if tag == target_tag {
                return Some(handle.clone());
            }
        }
        for child in handle.children.borrow().iter() {
            if let Some(found) = find_element(child, target_tag) {
                return Some(found);
            }
        }
        None
    }

    /// Rekursiv sökning efter element med specifikt attribut
    fn find_element_by_attr(handle: &Handle, attr: &str, val: &str) -> Option<Handle> {
        if let Some(v) = get_attr(handle, attr) {
            if v == val {
                return Some(handle.clone());
            }
        }
        for child in handle.children.borrow().iter() {
            if let Some(found) = find_element_by_attr(child, attr, val) {
                return Some(found);
            }
        }
        None
    }

    // === parse_html ===

    #[test]
    fn test_parse_simple_html() {
        let html = r#"<html><body><button>Klicka här</button></body></html>"#;
        let dom = parse_html(html);
        assert!(
            dom.document.children.borrow().len() > 0,
            "Tomt dokument efter parse"
        );
    }

    #[test]
    fn test_parse_malformed_html() {
        // html5ever ska hantera trasig HTML utan panik
        let html = r#"<html><body><div><p>Unclosed div<span>Nested</body>"#;
        let dom = parse_html(html);
        assert!(
            dom.document.children.borrow().len() > 0,
            "Malformed HTML ska ändå producera ett DOM-träd"
        );
    }

    #[test]
    fn test_parse_empty_html() {
        let dom = parse_html("");
        assert!(
            dom.document.children.borrow().len() > 0,
            "Tom HTML ska ge default DOM (html5ever lägger till html/head/body)"
        );
    }

    #[test]
    fn test_parse_utf8_multibyte() {
        let html = r#"<html><body><p>日本語テキスト 🎯 Ñoño</p></body></html>"#;
        let dom = parse_html(html);
        let p = find_element(&dom.document, "p").expect("Borde hitta <p>");
        let text = extract_text(&p);
        assert!(
            text.contains("日本語テキスト"),
            "Japansk text ska bevaras: got '{}'",
            text
        );
        assert!(text.contains("🎯"), "Emoji ska bevaras: got '{}'", text);
        assert!(
            text.contains("Ñoño"),
            "Latin-extended ska bevaras: got '{}'",
            text
        );
    }

    // === extract_text ===

    #[test]
    fn test_extract_text_nested() {
        let html = r#"<html><body><div>Yttre <span>inre</span> text</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        let text = extract_text(&div);
        assert!(
            text.contains("Yttre") && text.contains("inre") && text.contains("text"),
            "Ska extrahera text från alla nivåer: got '{}'",
            text
        );
    }

    #[test]
    fn test_extract_text_skips_script() {
        let html =
            r#"<html><body><div>Synlig text<script>var x = "dold";</script></div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        let text = extract_text(&div);
        assert!(
            text.contains("Synlig text"),
            "Synlig text ska finnas: got '{}'",
            text
        );
        assert!(
            !text.contains("dold"),
            "Script-innehåll ska INTE inkluderas: got '{}'",
            text
        );
    }

    #[test]
    fn test_extract_text_skips_style() {
        let html = r#"<html><body><div>Synlig<style>.hidden { display:none; }</style></div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        let text = extract_text(&div);
        assert!(
            !text.contains("display"),
            "Style-innehåll ska INTE inkluderas: got '{}'",
            text
        );
    }

    #[test]
    fn test_extract_text_whitespace_handling() {
        let html = r#"<html><body><p>   Massa    mellanslag   </p></body></html>"#;
        let dom = parse_html(html);
        let p = find_element(&dom.document, "p").expect("Borde hitta <p>");
        let text = extract_text(&p);
        let trimmed = text.trim();
        assert!(
            !trimmed.is_empty(),
            "Text ska extraheras trots extra whitespace"
        );
        assert!(
            trimmed.contains("Massa"),
            "Textinnehåll ska bevaras: got '{}'",
            trimmed
        );
    }

    // === get_attr ===

    #[test]
    fn test_get_attr_exists() {
        let html = r##"<html><body><a href="#top" class="nav-link">Länk</a></body></html>"##;
        let dom = parse_html(html);
        let a = find_element(&dom.document, "a").expect("Borde hitta <a>");
        assert_eq!(
            get_attr(&a, "href"),
            Some("#top".to_string()),
            "href ska extraheras korrekt"
        );
        assert_eq!(
            get_attr(&a, "class"),
            Some("nav-link".to_string()),
            "class ska extraheras korrekt"
        );
    }

    #[test]
    fn test_get_attr_missing() {
        let html = r#"<html><body><div>Ingen attr</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert_eq!(
            get_attr(&div, "id"),
            None,
            "Saknat attribut ska returnera None"
        );
    }

    // === get_tag_name ===

    #[test]
    fn test_get_tag_name() {
        let html = r#"<html><body><nav>Menu</nav></body></html>"#;
        let dom = parse_html(html);
        let nav = find_element(&dom.document, "nav").expect("Borde hitta <nav>");
        assert_eq!(
            get_tag_name(&nav),
            Some("nav".to_string()),
            "Taggnamn ska vara 'nav'"
        );
    }

    // === is_likely_visible ===

    #[test]
    fn test_visible_normal_element() {
        let html = r#"<html><body><button>Synlig</button></body></html>"#;
        let dom = parse_html(html);
        let btn = find_element(&dom.document, "button").expect("Borde hitta <button>");
        assert!(is_likely_visible(&btn), "Normal button ska vara synlig");
    }

    #[test]
    fn test_hidden_display_none() {
        let html = r#"<html><body><div style="display:none">Dold</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "display:none ska markera element som osynligt"
        );
    }

    #[test]
    fn test_hidden_visibility_hidden() {
        let html = r#"<html><body><div style="visibility: hidden">Dold</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "visibility:hidden ska markera element som osynligt"
        );
    }

    #[test]
    fn test_hidden_attribute() {
        let html = r#"<html><body><div hidden>Dold</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "hidden-attribut ska markera element som osynligt"
        );
    }

    #[test]
    fn test_hidden_aria_hidden_true() {
        let html = r#"<html><body><div aria-hidden="true">Osynlig</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "aria-hidden=true ska markera element som osynligt"
        );
    }

    #[test]
    fn test_hidden_opacity_zero() {
        let html = r#"<html><body><div style="opacity: 0;">Osynlig</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "opacity:0 ska markera element som osynligt"
        );
    }

    #[test]
    fn test_hidden_offscreen_left() {
        let html = r#"<html><body><div style="position:absolute;left:-9999px">Offscreen</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            !is_likely_visible(&div),
            "left:-9999px ska markera element som osynligt"
        );
    }

    #[test]
    fn test_aria_hidden_false_is_visible() {
        let html = r#"<html><body><div aria-hidden="false">Synlig</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            is_likely_visible(&div),
            "aria-hidden=false ska vara synligt"
        );
    }

    // === infer_role ===

    #[test]
    fn test_infer_role_aria_override() {
        let html = r#"<html><body><div role="navigation">Nav</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert_eq!(
            infer_role(&div),
            "navigation",
            "ARIA role ska ha högst prioritet"
        );
    }

    #[test]
    fn test_infer_role_input_types() {
        let cases = [
            ("checkbox", "checkbox"),
            ("radio", "radio"),
            ("submit", "button"),
            ("search", "searchbox"),
            ("text", "textbox"),
        ];
        for (input_type, expected_role) in cases {
            let html = format!(
                r#"<html><body><input type="{}" /></body></html>"#,
                input_type
            );
            let dom = parse_html(&html);
            let input = find_element(&dom.document, "input")
                .unwrap_or_else(|| panic!("Borde hitta <input type=\"{}\">", input_type));
            assert_eq!(
                infer_role(&input),
                expected_role,
                "input type='{}' ska ge roll '{}'",
                input_type,
                expected_role
            );
        }
    }

    #[test]
    fn test_infer_role_semantic_tags() {
        let cases = [
            ("nav", "navigation"),
            ("main", "main"),
            ("header", "banner"),
            ("footer", "contentinfo"),
            ("form", "form"),
            ("table", "table"),
            ("select", "combobox"),
            ("textarea", "textarea"),
        ];
        for (tag, expected_role) in cases {
            let html = format!(r#"<html><body><{}>Innehåll</{}></body></html>"#, tag, tag);
            let dom = parse_html(&html);
            let elem =
                find_element(&dom.document, tag).unwrap_or_else(|| panic!("Borde hitta <{}>", tag));
            assert_eq!(
                infer_role(&elem),
                expected_role,
                "<{}> ska ge roll '{}'",
                tag,
                expected_role
            );
        }
    }

    #[test]
    fn test_infer_role_headings() {
        for level in 1..=6 {
            let tag = format!("h{}", level);
            let html = format!(r#"<html><body><{}>Rubrik</{}></body></html>"#, tag, tag);
            let dom = parse_html(&html);
            let heading = find_element(&dom.document, &tag)
                .unwrap_or_else(|| panic!("Borde hitta <{}>", tag));
            assert_eq!(
                infer_role(&heading),
                "heading",
                "<{}> ska ge roll 'heading'",
                tag
            );
        }
    }

    #[test]
    fn test_infer_role_cta_keyword() {
        let html = r#"<html><body><button>Köp nu</button></body></html>"#;
        let dom = parse_html(html);
        let btn = find_element(&dom.document, "button").expect("Borde hitta <button>");
        assert_eq!(
            infer_role(&btn),
            "cta",
            "Button med CTA-nyckelord 'köp' ska ge roll 'cta'"
        );
    }

    #[test]
    fn test_infer_role_price_detection() {
        let html = r#"<html><body><span>199 kr</span></body></html>"#;
        let dom = parse_html(html);
        let span = find_element(&dom.document, "span").expect("Borde hitta <span>");
        assert_eq!(
            infer_role(&span),
            "price",
            "Text med valutaindikator ska ge roll 'price'"
        );
    }

    #[test]
    fn test_infer_role_product_card() {
        let html =
            r#"<html><body><div itemtype="https://schema.org/Product">Produkt</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert_eq!(
            infer_role(&div),
            "product_card",
            "Schema.org Product ska ge roll 'product_card'"
        );
    }

    // === extract_label (WCAG-kedja) ===

    #[test]
    fn test_aria_label_priority() {
        let html = r#"<html><body><button aria-label="Stäng dialog">X</button></body></html>"#;
        let dom = parse_html(html);
        let button = find_element(&dom.document, "button").expect("Borde hitta button");
        let label = extract_label(&button);
        assert_eq!(
            label, "Stäng dialog",
            "aria-label ska ha prioritet över inner text"
        );
    }

    #[test]
    fn test_label_aria_labelledby() {
        let html = r#"<html><body><button aria-labelledby="lbl1">X</button></body></html>"#;
        let dom = parse_html(html);
        let btn = find_element(&dom.document, "button").expect("Borde hitta <button>");
        let label = extract_label(&btn);
        assert_eq!(label, "[ref:lbl1]", "aria-labelledby ska ge ref-format");
    }

    #[test]
    fn test_label_placeholder_fallback() {
        let html = r#"<html><body><input placeholder="Sök produkter..." /></body></html>"#;
        let dom = parse_html(html);
        let input = find_element(&dom.document, "input").expect("Borde hitta <input>");
        let label = extract_label(&input);
        assert_eq!(
            label, "Sök produkter...",
            "placeholder ska användas som fallback-label"
        );
    }

    #[test]
    fn test_label_alt_text() {
        let html = r#"<html><body><img alt="Produktbild av stol" /></body></html>"#;
        let dom = parse_html(html);
        let img = find_element(&dom.document, "img").expect("Borde hitta <img>");
        let label = extract_label(&img);
        assert_eq!(
            label, "Produktbild av stol",
            "alt-text ska användas som label för bilder"
        );
    }

    #[test]
    fn test_label_title_fallback() {
        let html = r#"<html><body><div title="Verktygstips">...</div></body></html>"#;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        let label = extract_label(&div);
        assert_eq!(
            label, "Verktygstips",
            "title-attribut ska vara fallback-label"
        );
    }

    #[test]
    fn test_label_inner_text_fallback() {
        let html = r#"<html><body><button>Skicka formulär</button></body></html>"#;
        let dom = parse_html(html);
        let btn = find_element(&dom.document, "button").expect("Borde hitta <button>");
        let label = extract_label(&btn);
        assert!(
            label.contains("Skicka formulär"),
            "Inner text ska vara sista label-fallback: got '{}'",
            label
        );
    }

    #[test]
    fn test_label_name_attribute_last_resort() {
        let html = r#"<html><body><input name="email" /></body></html>"#;
        let dom = parse_html(html);
        let input = find_element(&dom.document, "input").expect("Borde hitta <input>");
        let label = extract_label(&input);
        assert_eq!(
            label, "email",
            "name-attribut ska vara sista utväg för label"
        );
    }

    #[test]
    fn test_label_truncation_300_chars() {
        let long_text = "A".repeat(400);
        let html = format!(
            r#"<html><body><button>{}</button></body></html>"#,
            long_text
        );
        let dom = parse_html(&html);
        let btn = find_element(&dom.document, "button").expect("Borde hitta <button>");
        let label = extract_label(&btn);
        assert!(
            label.len() <= 300,
            "Label ska trunkeras till max 300 tecken, got {} tecken",
            label.len()
        );
        // Ska vara mer än 80 (gamla gränsen)
        assert!(
            label.len() > 80,
            "Label ska tillåta >80 tecken nu, got {} tecken",
            label.len()
        );
    }

    // === resolve_lazy_src ===

    #[test]
    fn test_resolve_lazy_src_data_src() {
        let html = r##"<html><body><img data-src="https://example.com/photo.jpg" alt="Foto" /></body></html>"##;
        let dom = parse_html(html);
        let img = find_element(&dom.document, "img").expect("Borde hitta <img>");
        let src = resolve_lazy_src(&img);
        assert_eq!(
            src,
            Some("https://example.com/photo.jpg".to_string()),
            "data-src borde resolveas som bild-URL"
        );
    }

    #[test]
    fn test_resolve_lazy_src_data_lazy_src() {
        let html = r##"<html><body><img data-lazy-src="https://cdn.example.com/img.webp" /></body></html>"##;
        let dom = parse_html(html);
        let img = find_element(&dom.document, "img").expect("Borde hitta <img>");
        let src = resolve_lazy_src(&img);
        assert_eq!(
            src,
            Some("https://cdn.example.com/img.webp".to_string()),
            "data-lazy-src borde resolveas"
        );
    }

    #[test]
    fn test_resolve_lazy_src_placeholder_data_uri() {
        let html = r##"<html><body><img src="data:image/svg+xml;base64,PHN2Zz4=" data-src="https://real.com/img.png" /></body></html>"##;
        let dom = parse_html(html);
        let img = find_element(&dom.document, "img").expect("Borde hitta <img>");
        let src = resolve_lazy_src(&img);
        assert_eq!(
            src,
            Some("https://real.com/img.png".to_string()),
            "Placeholder data-URI borde ignoreras, data-src borde resolveas"
        );
    }

    #[test]
    fn test_resolve_lazy_src_real_src_not_overridden() {
        let html = r##"<html><body><img src="https://real.com/img.png" data-src="https://other.com/lazy.png" /></body></html>"##;
        let dom = parse_html(html);
        let img = find_element(&dom.document, "img").expect("Borde hitta <img>");
        let src = resolve_lazy_src(&img);
        assert_eq!(
            src,
            Some("https://real.com/img.png".to_string()),
            "Riktig src borde inte overridas av data-src"
        );
    }

    #[test]
    fn test_resolve_lazy_src_non_img_returns_none() {
        let html =
            r##"<html><body><div data-src="https://example.com/bg.jpg">Text</div></body></html>"##;
        let dom = parse_html(html);
        let div = find_element(&dom.document, "div").expect("Borde hitta <div>");
        assert!(
            resolve_lazy_src(&div).is_none(),
            "resolve_lazy_src borde returnera None för icke-img-element"
        );
    }

    // === nojs-filtrering ===

    #[test]
    fn test_nojs_div_hidden_by_id() {
        let html = r##"<html><body><div id="nojs">Aktivera JavaScript</div></body></html>"##;
        let dom = parse_html(html);
        let div = find_element_by_attr(&dom.document, "id", "nojs").expect("Borde hitta div#nojs");
        assert!(
            !is_likely_visible(&div),
            "div#nojs borde markeras som osynlig"
        );
    }

    #[test]
    fn test_nojs_div_hidden_by_class() {
        let html =
            r##"<html><body><div class="no-js warning">Aktivera JavaScript</div></body></html>"##;
        let dom = parse_html(html);
        let div = find_element_by_attr(&dom.document, "class", "no-js warning")
            .expect("Borde hitta div.no-js");
        assert!(
            !is_likely_visible(&div),
            "div.no-js borde markeras som osynlig"
        );
    }

    #[test]
    fn test_normal_div_still_visible() {
        let html = r##"<html><body><div id="content">Innehåll</div></body></html>"##;
        let dom = parse_html(html);
        let div =
            find_element_by_attr(&dom.document, "id", "content").expect("Borde hitta div#content");
        assert!(is_likely_visible(&div), "Vanlig div borde vara synlig");
    }
}
