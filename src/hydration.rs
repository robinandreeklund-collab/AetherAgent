/// Hydration State Extraction (Tier 0)
///
/// Extraherar SSR-hydration-data från HTML utan att köra JavaScript.
/// Stöder 10+ ramverk: Next.js (Pages + App Router), Nuxt, Angular Universal,
/// Remix, Gatsby, SvelteKit, Qwik, Astro, Apollo GraphQL.
///
/// Täcker ~40% av moderna produktionssidor → 0 ms JS-exekvering.
use serde_json::Value;

use crate::trust::{analyze_text, sanitize_text};
use crate::types::{InjectionWarning, NodeState, SemanticNode, TrustLevel};

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Detekterat ramverk
#[derive(Debug, Clone, PartialEq)]
pub enum Framework {
    NextJs,
    NextFlight,
    Nuxt,
    Angular,
    Remix,
    Gatsby,
    SvelteKit,
    Qwik,
    Astro,
    Apollo,
}

/// Extraherad hydration-data
#[derive(Debug, Clone)]
pub struct HydrationData {
    pub framework: Framework,
    pub props: Value,
}

/// Resultat från hydration-extraction pipelinen
#[derive(Debug)]
pub struct HydrationResult {
    pub data: HydrationData,
    pub nodes: Vec<SemanticNode>,
    pub warnings: Vec<InjectionWarning>,
}

// ─── Huvudfunktion ──────────────────────────────────────────────────────────

/// Försöker extrahera hydration-data från HTML. Returnerar None om ingen
/// känd ramverks-data hittades.
pub fn extract_hydration_state(html: &str) -> Option<HydrationData> {
    // Prova varje ramverk i prioritetsordning (vanligast först)
    if let Some(data) = extract_next_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_next_flight(html) {
        return Some(data);
    }
    if let Some(data) = extract_nuxt_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_angular_state(html) {
        return Some(data);
    }
    if let Some(data) = extract_remix_context(html) {
        return Some(data);
    }
    if let Some(data) = extract_gatsby_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_sveltekit_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_qwik_state(html) {
        return Some(data);
    }
    if let Some(data) = extract_astro_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_apollo_state(html) {
        return Some(data);
    }
    None
}

/// Konvertera HydrationData till semantiska noder.
/// Bygger ett platt träd av text/data-noder med trust shield.
pub fn hydration_to_nodes(data: &HydrationData, goal: &str) -> HydrationResult {
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();
    let mut next_id: u32 = 0;

    // Extrahera key-value-par från JSON-props
    let entries = flatten_json_to_entries(&data.props, "", 32);

    let goal_lower = goal.to_lowercase();
    let goal_words: Vec<&str> = goal_lower
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|s| !s.is_empty())
        .collect();

    for (key, value) in &entries {
        // Skippa interna/metadata-nycklar
        if is_internal_key(key) {
            continue;
        }

        let label = format!("{}: {}", key, value);

        // Trust shield — analysera text
        let (trust, warning) = analyze_text(next_id, &label);
        if let Some(w) = warning {
            warnings.push(w);
        }

        let sanitized_label = if trust != TrustLevel::Untrusted {
            label.clone()
        } else {
            sanitize_text(&label)
        };

        // Beräkna relevans mot målet
        let relevance = score_entry_relevance(key, value, &goal_words);

        let node = SemanticNode {
            id: next_id,
            role: "data".to_string(),
            label: sanitized_label,
            value: Some(value.clone()),
            state: NodeState::default_state(),
            action: None,
            relevance,
            trust,
            children: vec![],
            html_id: None,
            name: Some(key.clone()),
            bbox: None,
        };
        nodes.push(node);
        next_id += 1;
    }

    HydrationResult {
        data: data.clone(),
        nodes,
        warnings,
    }
}

// ─── Ramverks-extraktorer ───────────────────────────────────────────────────

/// Next.js Pages Router: `<script id="__NEXT_DATA__" type="application/json">`
fn extract_next_data(html: &str) -> Option<HydrationData> {
    let marker = r#"id="__NEXT_DATA__""#;
    let pos = html.find(marker)?;

    // Hitta > efter markören
    let rest = html.get(pos..)?;
    let tag_end = rest.find('>')?;
    let after_tag = rest.get(tag_end + 1..)?;

    // Hitta </script>
    let end = after_tag.find("</script>")?;
    let json_str = after_tag.get(..end)?.trim();

    let parsed: Value = serde_json::from_str(json_str).ok()?;

    // Extrahera props.pageProps om den finns
    let props = parsed
        .get("props")
        .and_then(|p| p.get("pageProps"))
        .cloned()
        .unwrap_or(parsed);

    Some(HydrationData {
        framework: Framework::NextJs,
        props,
    })
}

/// Next.js App Router (React Flight Protocol): `self.__next_f.push([...])`
fn extract_next_flight(html: &str) -> Option<HydrationData> {
    let marker = "self.__next_f.push(";
    if !html.contains(marker) {
        return None;
    }

    let mut collected = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = html[search_from..].find(marker) {
        let abs_pos = search_from + pos;
        let rest = html.get(abs_pos + marker.len()..)?;
        // Hitta matchande )
        if let Some(end) = find_balanced_paren(rest) {
            let chunk = rest.get(..end)?;
            // Parsea som JSON-array [typ, data]
            if let Ok(arr) = serde_json::from_str::<Value>(chunk) {
                if let Some(arr) = arr.as_array() {
                    // Typ 1 = data-chunk
                    if arr.len() >= 2 {
                        if let Some(s) = arr[1].as_str() {
                            collected.push(s.to_string());
                        }
                    }
                }
            }
            search_from = abs_pos + marker.len() + end;
        } else {
            break;
        }
    }

    if collected.is_empty() {
        return None;
    }

    // Försök parsea ihopslagna chunks som JSON
    let combined = collected.join("");
    let props = match serde_json::from_str::<Value>(&combined) {
        Ok(v) => v,
        Err(_) => Value::String(combined),
    };

    Some(HydrationData {
        framework: Framework::NextFlight,
        props,
    })
}

/// Nuxt.js: `window.__NUXT__=` eller `<script id="__NUXT_DATA__">`
fn extract_nuxt_data(html: &str) -> Option<HydrationData> {
    // Nuxt 3: id="__NUXT_DATA__"
    if let Some(data) = extract_script_by_id(html, "__NUXT_DATA__") {
        let props = serde_json::from_str::<Value>(&data).ok()?;
        return Some(HydrationData {
            framework: Framework::Nuxt,
            props,
        });
    }

    // Nuxt 2: window.__NUXT__=
    let marker = "window.__NUXT__=";
    let pos = html.find(marker)?;
    let rest = html.get(pos + marker.len()..)?;
    let end = find_js_object_end(rest)?;
    let json_str = rest.get(..end)?;
    let props = serde_json::from_str::<Value>(json_str).ok()?;

    Some(HydrationData {
        framework: Framework::Nuxt,
        props,
    })
}

/// Angular Universal: `<script id="ng-state" type="application/json">`
fn extract_angular_state(html: &str) -> Option<HydrationData> {
    let data = extract_script_by_id(html, "ng-state")?;
    let props = serde_json::from_str::<Value>(&data).ok()?;

    Some(HydrationData {
        framework: Framework::Angular,
        props,
    })
}

/// Remix: `window.__remixContext = `
fn extract_remix_context(html: &str) -> Option<HydrationData> {
    let marker = "window.__remixContext";
    let pos = html.find(marker)?;
    let rest = html.get(pos + marker.len()..)?;

    // Skippa whitespace och =
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start();

    let end = find_js_object_end(rest)?;
    let json_str = rest.get(..end)?;
    let parsed = serde_json::from_str::<Value>(json_str).ok()?;

    // Extrahera loaderData om tillgängligt
    let props = parsed
        .get("state")
        .and_then(|s| s.get("loaderData"))
        .cloned()
        .unwrap_or(parsed);

    Some(HydrationData {
        framework: Framework::Remix,
        props,
    })
}

/// Gatsby: `<script id="___gatsby-initial-props">`
fn extract_gatsby_data(html: &str) -> Option<HydrationData> {
    // Gatsby SSR har page-data i ett script-tag
    let data = extract_script_by_id(html, "___gatsby-initial-props")
        .or_else(|| extract_script_by_id(html, "__gatsby-initial-props"))?;
    let props = serde_json::from_str::<Value>(&data).ok()?;

    Some(HydrationData {
        framework: Framework::Gatsby,
        props,
    })
}

/// SvelteKit: `<script id="__sveltekit_data" type="application/json">`
fn extract_sveltekit_data(html: &str) -> Option<HydrationData> {
    // SvelteKit använder __sveltekit_data eller sveltekit:data
    let data = extract_script_by_id(html, "__sveltekit_data")
        .or_else(|| extract_script_by_id(html, "svelte-announcer"))?;

    let props = serde_json::from_str::<Value>(&data).ok()?;

    Some(HydrationData {
        framework: Framework::SvelteKit,
        props,
    })
}

/// Qwik: `<script type="qwik/json">`
fn extract_qwik_state(html: &str) -> Option<HydrationData> {
    let marker = r#"type="qwik/json""#;
    let pos = html.find(marker)?;

    let rest = html.get(pos..)?;
    let tag_end = rest.find('>')?;
    let after_tag = rest.get(tag_end + 1..)?;

    let end = after_tag.find("</script>")?;
    let json_str = after_tag.get(..end)?.trim();

    let props = serde_json::from_str::<Value>(json_str).ok()?;

    Some(HydrationData {
        framework: Framework::Qwik,
        props,
    })
}

/// Astro: `<script type="application/json" data-astro-transition>`
/// eller `<astro-island props="...">`
fn extract_astro_data(html: &str) -> Option<HydrationData> {
    // Astro Islands: <astro-island ... props="...">
    let marker = "<astro-island";
    let pos = html.find(marker)?;
    let rest = html.get(pos..)?;

    // Hitta props-attributet
    let props_marker = "props=\"";
    let props_pos = rest.find(props_marker)?;
    let after_props = rest.get(props_pos + props_marker.len()..)?;

    // Hitta avslutande "
    let end_quote = after_props.find('"')?;
    let encoded_props = after_props.get(..end_quote)?;

    // Astro HTML-kodar JSON i attributet
    let decoded = encoded_props
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");

    let props = serde_json::from_str::<Value>(&decoded).ok()?;

    Some(HydrationData {
        framework: Framework::Astro,
        props,
    })
}

/// Apollo GraphQL: `window.__APOLLO_STATE__`
fn extract_apollo_state(html: &str) -> Option<HydrationData> {
    let marker = "window.__APOLLO_STATE__";
    let pos = html.find(marker)?;
    let rest = html.get(pos + marker.len()..)?;

    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start();

    let end = find_js_object_end(rest)?;
    let json_str = rest.get(..end)?;
    let props = serde_json::from_str::<Value>(json_str).ok()?;

    Some(HydrationData {
        framework: Framework::Apollo,
        props,
    })
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Extrahera innehåll ur `<script id="ID">...innehåll...</script>`
fn extract_script_by_id(html: &str, id: &str) -> Option<String> {
    // Sök efter id="ID" (med dubbla citattecken)
    let pattern = format!(r#"id="{}""#, id);
    let pos = html.find(&pattern)?;

    let rest = html.get(pos..)?;
    let tag_end = rest.find('>')?;
    let after_tag = rest.get(tag_end + 1..)?;

    let end = after_tag.find("</script>")?;
    let content = after_tag.get(..end)?.trim();

    Some(content.to_string())
}

/// Hitta slutet av ett JS-objekt/array (balanserade { } eller [ ])
fn find_js_object_end(s: &str) -> Option<usize> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let first = s.as_bytes().first()?;
    let (open, close) = match first {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in s.bytes().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if ch == b'\\' && in_string {
            escape = true;
            continue;
        }
        if ch == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Hitta matchande parentes (balanserade paranteser)
fn find_balanced_paren(s: &str) -> Option<usize> {
    // Startposition: vi förväntar oss att s börjar precis efter öppnande (
    // Vi letar efter matchande )
    let mut depth = 1i32;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in s.bytes().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if ch == b'\\' && in_string {
            escape = true;
            continue;
        }
        if ch == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == b'(' {
            depth += 1;
        } else if ch == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Plattar ut JSON till key-value-par, max depth nivåer djupt
fn flatten_json_to_entries(value: &Value, prefix: &str, max_depth: u32) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    if max_depth == 0 {
        entries.push((prefix.to_string(), format_value(value)));
        return entries;
    }

    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                entries.extend(flatten_json_to_entries(val, &full_key, max_depth - 1));
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let full_key = if prefix.is_empty() {
                    format!("[{}]", i)
                } else {
                    format!("{}[{}]", prefix, i)
                };
                // Plattar bara primitiva array-element
                if val.is_object() || val.is_array() {
                    entries.extend(flatten_json_to_entries(val, &full_key, max_depth - 1));
                } else {
                    entries.push((full_key, format_value(val)));
                }
            }
        }
        _ => {
            entries.push((prefix.to_string(), format_value(value)));
        }
    }

    entries
}

/// Formatera JSON-värde till sträng
fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// Nycklar som ska skippas (ramverk-interna, metadata)
fn is_internal_key(key: &str) -> bool {
    let internal_prefixes = [
        "__N_",          // Next.js intern
        "_sentryTrace",  // Sentry
        "buildId",       // Next.js
        "assetPrefix",   // Next.js
        "runtimeConfig", // Nuxt runtime
        "_resolved",     // Qwik
        "__typename",    // Apollo/GraphQL
    ];

    for prefix in &internal_prefixes {
        if key.starts_with(prefix) || key.contains(prefix) {
            return true;
        }
    }
    false
}

/// Poängsätt hur relevant en entry är mot agentens mål
fn score_entry_relevance(key: &str, value: &str, goal_words: &[&str]) -> f32 {
    if goal_words.is_empty() {
        return 0.5; // Neutral om inget mål
    }

    let key_lower = key.to_lowercase();
    let value_lower = value.to_lowercase();

    let mut matches = 0;
    for word in goal_words {
        if word.len() < 2 {
            continue;
        }
        if key_lower.contains(word) || value_lower.contains(word) {
            matches += 1;
        }
    }

    let ratio = matches as f32 / goal_words.len() as f32;
    // Skala till 0.1–1.0 (alltid minst lite relevans för hydration-data)
    0.1 + ratio * 0.9
}

// ─── Tester ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // === Next.js Pages Router ===

    #[test]
    fn test_extract_next_data_basic() {
        let html = r##"
        <html><head></head><body>
        <div id="__next">Innehåll</div>
        <script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"title":"Hem","products":[{"id":1,"name":"Produkt A","price":299}]}},"page":"/"}
        </script>
        </body></html>
        "##;

        let result = extract_hydration_state(html);
        assert!(result.is_some(), "Borde hitta Next.js hydration data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::NextJs, "Borde vara NextJs");
        assert_eq!(
            data.props.get("title").and_then(|v| v.as_str()),
            Some("Hem"),
            "Borde extrahera pageProps.title"
        );
        assert!(
            data.props.get("products").is_some(),
            "Borde extrahera products"
        );
    }

    #[test]
    fn test_extract_next_data_missing() {
        let html = "<html><body><p>Ingen hydration</p></body></html>";
        assert!(
            extract_hydration_state(html).is_none(),
            "Borde returnera None utan hydration data"
        );
    }

    #[test]
    fn test_extract_next_data_invalid_json() {
        let html = r##"
        <script id="__NEXT_DATA__" type="application/json">
        {invalid json here}
        </script>
        "##;
        assert!(
            extract_next_data(html).is_none(),
            "Borde returnera None vid ogiltig JSON"
        );
    }

    // === Next.js App Router (Flight) ===

    #[test]
    fn test_extract_next_flight() {
        let html = r#"
        <script>self.__next_f.push([1,"\"hello\""])</script>
        <script>self.__next_f.push([1,"\"world\""])</script>
        "#;

        let result = extract_next_flight(html);
        assert!(result.is_some(), "Borde hitta Next.js Flight data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::NextFlight);
    }

    // === Nuxt ===

    #[test]
    fn test_extract_nuxt_data_v2() {
        let html = r#"
        <script>window.__NUXT__={"data":[{"items":["a","b"]}],"state":{"count":42}}</script>
        "#;

        let result = extract_nuxt_data(html);
        assert!(result.is_some(), "Borde hitta Nuxt hydration data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Nuxt);
        assert!(data.props.get("data").is_some(), "Borde ha data-fält");
    }

    #[test]
    fn test_extract_nuxt_data_v3() {
        let html = r##"
        <script id="__NUXT_DATA__" type="application/json">
        {"data":{"message":"hej"},"state":{}}
        </script>
        "##;

        let result = extract_nuxt_data(html);
        assert!(result.is_some(), "Borde hitta Nuxt 3 data");
    }

    // === Angular ===

    #[test]
    fn test_extract_angular_state() {
        let html = r##"
        <script id="ng-state" type="application/json">
        {"API_DATA":{"users":[{"name":"Anna","email":"anna@test.se"}]}}
        </script>
        "##;

        let result = extract_angular_state(html);
        assert!(result.is_some(), "Borde hitta Angular state");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Angular);
    }

    // === Remix ===

    #[test]
    fn test_extract_remix_context() {
        let html = r#"
        <script>window.__remixContext = {"state":{"loaderData":{"root":{"user":"Erik"}}}}</script>
        "#;

        let result = extract_remix_context(html);
        assert!(result.is_some(), "Borde hitta Remix context");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Remix);
        // Borde extrahera loaderData
        assert!(
            data.props.get("root").is_some(),
            "Borde extrahera loaderData.root"
        );
    }

    // === Gatsby ===

    #[test]
    fn test_extract_gatsby_data() {
        let html = r##"
        <script id="___gatsby-initial-props" type="application/json">
        {"pageContext":{"slug":"/blogg","title":"Min blogg"}}
        </script>
        "##;

        let result = extract_gatsby_data(html);
        assert!(result.is_some(), "Borde hitta Gatsby data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Gatsby);
    }

    // === SvelteKit ===

    #[test]
    fn test_extract_sveltekit_data() {
        let html = r##"
        <script id="__sveltekit_data" type="application/json">
        {"type":"data","nodes":[{"type":"data","data":{"items":["x","y"]}}]}
        </script>
        "##;

        let result = extract_sveltekit_data(html);
        assert!(result.is_some(), "Borde hitta SvelteKit data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::SvelteKit);
    }

    // === Qwik ===

    #[test]
    fn test_extract_qwik_state() {
        let html = r#"
        <script type="qwik/json">
        {"ctx":{"items":[1,2,3]},"objs":["hello"]}
        </script>
        "#;

        let result = extract_qwik_state(html);
        assert!(result.is_some(), "Borde hitta Qwik state");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Qwik);
    }

    // === Astro ===

    #[test]
    fn test_extract_astro_data() {
        let html = r#"
        <astro-island component-url="/Counter" props="{ &quot;count&quot;: 0, &quot;label&quot;: &quot;Klick&quot; }">
        </astro-island>
        "#;

        let result = extract_astro_data(html);
        assert!(result.is_some(), "Borde hitta Astro island data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Astro);
    }

    // === Apollo ===

    #[test]
    fn test_extract_apollo_state() {
        let html = r#"
        <script>window.__APOLLO_STATE__ = {"ROOT_QUERY":{"products":[{"id":"1","name":"Sko"}]}}</script>
        "#;

        let result = extract_apollo_state(html);
        assert!(result.is_some(), "Borde hitta Apollo state");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Apollo);
    }

    // === Dispatcher ===

    #[test]
    fn test_dispatcher_priority() {
        // Next.js borde ha prioritet över andra ramverk om båda finns
        let html = r##"
        <script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"x":1}},"page":"/"}
        </script>
        <script>window.__NUXT__={"data":[]}</script>
        "##;

        let result = extract_hydration_state(html);
        assert!(result.is_some(), "Borde hitta data");
        assert_eq!(
            result.unwrap().framework,
            Framework::NextJs,
            "Next.js borde ha prioritet"
        );
    }

    // === Hydration till noder ===

    #[test]
    fn test_hydration_to_nodes() {
        let data = HydrationData {
            framework: Framework::NextJs,
            props: serde_json::json!({
                "title": "Testprodukt",
                "price": 199,
                "description": "En bra produkt"
            }),
        };

        let result = hydration_to_nodes(&data, "köpa produkt");

        assert!(
            !result.nodes.is_empty(),
            "Borde generera noder från hydration data"
        );

        // Alla noder borde ha roll "data"
        for node in &result.nodes {
            assert_eq!(node.role, "data", "Hydration-noder borde ha roll 'data'");
            assert!(node.value.is_some(), "Borde ha value");
        }

        // Noder relaterade till "produkt" borde ha högre relevans
        let product_node = result
            .nodes
            .iter()
            .find(|n| n.label.contains("description"));
        assert!(product_node.is_some(), "Borde hitta description-nod");
        let product_rel = product_node.unwrap().relevance;
        assert!(
            product_rel > 0.1,
            "Produkt-relaterad nod borde ha relevans > 0.1, fick {}",
            product_rel
        );
    }

    #[test]
    fn test_hydration_skips_internal_keys() {
        let data = HydrationData {
            framework: Framework::NextJs,
            props: serde_json::json!({
                "title": "Test",
                "buildId": "abc123",
                "__N_SSP": true,
                "_sentryTraceData": "xyz"
            }),
        };

        let result = hydration_to_nodes(&data, "");
        let keys: Vec<&str> = result
            .nodes
            .iter()
            .filter_map(|n| n.name.as_deref())
            .collect();

        assert!(!keys.contains(&"buildId"), "Borde skippa buildId");
        assert!(!keys.contains(&"__N_SSP"), "Borde skippa __N_ prefix");
        assert!(keys.contains(&"title"), "Borde behålla title");
    }

    // === Hjälpfunktioner ===

    #[test]
    fn test_find_js_object_end() {
        assert_eq!(find_js_object_end(r#"{"a":1}"#), Some(7));
        assert_eq!(find_js_object_end(r#"{"a":"}"}"#), Some(9));
        assert_eq!(find_js_object_end("[1,2,3]"), Some(7));
        assert_eq!(find_js_object_end(r#"{"nested":{"b":2}}"#), Some(18));
        assert!(find_js_object_end("not json").is_none());
    }

    #[test]
    fn test_flatten_json() {
        let json = serde_json::json!({
            "name": "Test",
            "nested": {
                "value": 42
            },
            "list": [1, 2]
        });

        let entries = flatten_json_to_entries(&json, "", 10);
        let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();

        assert!(keys.contains(&"name"), "Borde ha 'name'");
        assert!(keys.contains(&"nested.value"), "Borde ha 'nested.value'");
        assert!(keys.contains(&"list[0]"), "Borde ha 'list[0]'");
    }

    #[test]
    fn test_score_entry_relevance_with_goal() {
        let goal_words = vec!["buy", "product"];
        assert!(
            score_entry_relevance("product_name", "Nike Air", &goal_words) > 0.3,
            "Nyckel med goal-ord borde ha hög relevans"
        );
        assert!(
            score_entry_relevance("random_field", "random_value", &goal_words) < 0.2,
            "Orelaterad nyckel borde ha låg relevans"
        );
    }

    #[test]
    fn test_score_entry_relevance_empty_goal() {
        let goal_words: Vec<&str> = vec![];
        let score = score_entry_relevance("anything", "whatever", &goal_words);
        assert!((score - 0.5).abs() < f32::EPSILON, "Tomt mål borde ge 0.5");
    }
}
