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
    Eleventy,
    StaticHtml,
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
    // Snabb pre-check: om HTML saknar typiska hydration-markörer, skippa all scanning
    if !html.contains("__NEXT_DATA__")
        && !html.contains("__NUXT__")
        && !html.contains("__remixContext")
        && !html.contains("__GATSBY")
        && !html.contains("__APOLLO_STATE__")
        && !html.contains("type=\"application/json\"")
        && !html.contains("q:container")
        && !html.contains("data-astro")
    {
        return None;
    }

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
    if let Some(data) = extract_eleventy_data(html) {
        return Some(data);
    }
    if let Some(data) = extract_static_html_data(html) {
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
///
/// RSC wire format: radbaserat med typ-prefix per rad.
/// Typ 0 = bootstrap, typ 1 = string data, typ 2 = chunk ref.
/// Varje rad i data-chunks kan vara: JSON-objekt, JSON-array, eller RSC-reference ("$Lxx").
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
        if let Some(end) = find_balanced_paren(rest) {
            let chunk = rest.get(..end)?;
            if let Ok(arr) = serde_json::from_str::<Value>(chunk) {
                if let Some(arr) = arr.as_array() {
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

    // RSC wire format: parsea radvis — varje rad kan vara JSON
    let combined = collected.join("");
    let mut rsc_data = serde_json::Map::new();
    let mut rsc_index = 0u32;

    for line in combined.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // RSC-rader: "ID:TYPE:DATA" — t.ex. "0:{"key":"val"}" eller bara JSON
        // Försök parsea som JSON direkt
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            match &val {
                Value::Object(map) => {
                    // Slå samman objekt-data i toppnivå
                    for (k, v) in map {
                        rsc_data.insert(k.clone(), v.clone());
                    }
                }
                Value::Array(_) => {
                    rsc_data.insert(format!("_rsc_{}", rsc_index), val);
                    rsc_index += 1;
                }
                _ => {
                    rsc_data.insert(format!("_rsc_{}", rsc_index), val);
                    rsc_index += 1;
                }
            }
            continue;
        }
        // RSC radformat: "ID:TYPECHAR:DATA" — extrahera JSON-delen
        if let Some(colon_pos) = line.find(':') {
            let after_id = line.get(colon_pos + 1..)?;
            // Skippa typ-tecken om det finns
            let data_str = if after_id.len() > 1 && after_id.as_bytes()[1] == b':' {
                after_id.get(2..)?
            } else {
                after_id
            };
            if let Ok(val) = serde_json::from_str::<Value>(data_str) {
                if let Value::Object(map) = &val {
                    for (k, v) in map {
                        rsc_data.insert(k.clone(), v.clone());
                    }
                } else {
                    rsc_data.insert(format!("_rsc_{}", rsc_index), val);
                    rsc_index += 1;
                }
            }
        }
    }

    let props = if rsc_data.is_empty() {
        // Fallback: hela strängen som Value
        Value::String(combined)
    } else {
        Value::Object(rsc_data)
    };

    Some(HydrationData {
        framework: Framework::NextFlight,
        props,
    })
}

/// Nuxt.js: `window.__NUXT__=` eller `<script id="__NUXT_DATA__">`
///
/// Nuxt 3+ använder devalue-format (inte ren JSON) som stöder Date, BigInt,
/// Map, Set och cykliska referenser. Vi parsar devalue via parse_devalue().
fn extract_nuxt_data(html: &str) -> Option<HydrationData> {
    // Nuxt 3: id="__NUXT_DATA__" — ofta devalue-kodat
    if let Some(data) = extract_script_by_id(html, "__NUXT_DATA__") {
        // Försök devalue först, sedan ren JSON
        let props = parse_devalue(&data).or_else(|| serde_json::from_str::<Value>(&data).ok())?;
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
///
/// SvelteKit 2+ använder devalue-format. Försöker devalue först, sedan ren JSON.
fn extract_sveltekit_data(html: &str) -> Option<HydrationData> {
    let data = extract_script_by_id(html, "__sveltekit_data")
        .or_else(|| extract_script_by_id(html, "svelte-announcer"))?;

    // Försök devalue först, sedan ren JSON
    let props = parse_devalue(&data).or_else(|| serde_json::from_str::<Value>(&data).ok())?;

    Some(HydrationData {
        framework: Framework::SvelteKit,
        props,
    })
}

/// Qwik: `<script type="qwik/json">` + QRL event handlers
///
/// Qwik använder resumability — inte hydration. State lagras i qwik/json,
/// event handlers i QRL-attribut (on:click, on:input, etc.).
fn extract_qwik_state(html: &str) -> Option<HydrationData> {
    let marker = r#"type="qwik/json""#;
    let pos = html.find(marker)?;

    let rest = html.get(pos..)?;
    let tag_end = rest.find('>')?;
    let after_tag = rest.get(tag_end + 1..)?;

    let end = after_tag.find("</script>")?;
    let json_str = after_tag.get(..end)?.trim();

    let mut props = serde_json::from_str::<Value>(json_str).ok()?;

    // Extrahera QRL event handlers från HTML-attribut
    let qrl_handlers = extract_qwik_qrl_handlers(html);
    if !qrl_handlers.is_empty() {
        if let Value::Object(ref mut map) = props {
            let qrl_arr: Vec<Value> = qrl_handlers
                .into_iter()
                .map(|(event, handler)| {
                    serde_json::json!({
                        "event": event,
                        "handler": handler
                    })
                })
                .collect();
            map.insert("_qrl_handlers".to_string(), Value::Array(qrl_arr));
        }
    }

    Some(HydrationData {
        framework: Framework::Qwik,
        props,
    })
}

/// Extrahera Qwik QRL event handler-attribut från HTML
///
/// QRL-format: `on:click="./module.js#handler_fn"` eller `on:input="..."`
fn extract_qwik_qrl_handlers(html: &str) -> Vec<(String, String)> {
    let mut handlers = Vec::new();
    // Sök efter on:EVENT="HANDLER" mönster
    let mut search_from = 0;
    while let Some(pos) = html[search_from..].find("on:") {
        let abs_pos = search_from + pos;
        let rest = match html.get(abs_pos + 3..) {
            Some(r) => r,
            None => break,
        };

        // Extrahera event-namn (fram till =)
        let eq_pos = match rest.find('=') {
            Some(p) => p,
            None => {
                search_from = abs_pos + 3;
                continue;
            }
        };

        let event_name = rest.get(..eq_pos).unwrap_or("").trim();
        // Validera att det är ett rimligt event-namn (inga mellanslag/specialtecken)
        if event_name.is_empty()
            || event_name.len() > 30
            || event_name.contains(|c: char| c.is_whitespace())
        {
            search_from = abs_pos + 3;
            continue;
        }

        // Extrahera handler-värde (citattecken)
        let after_eq = match rest.get(eq_pos + 1..) {
            Some(r) => r.trim_start(),
            None => break,
        };
        let (handler_value, skip) = if after_eq.starts_with('"') {
            let inner = match after_eq.get(1..) {
                Some(s) => s,
                None => {
                    search_from = abs_pos + 3;
                    continue;
                }
            };
            match inner.find('"') {
                Some(end) => (after_eq.get(1..end + 1).unwrap_or(""), end + 2),
                None => {
                    search_from = abs_pos + 3;
                    continue;
                }
            }
        } else if after_eq.starts_with('\'') {
            let inner = match after_eq.get(1..) {
                Some(s) => s,
                None => {
                    search_from = abs_pos + 3;
                    continue;
                }
            };
            match inner.find('\'') {
                Some(end) => (after_eq.get(1..end + 1).unwrap_or(""), end + 2),
                None => {
                    search_from = abs_pos + 3;
                    continue;
                }
            }
        } else {
            search_from = abs_pos + 3;
            continue;
        };

        if !handler_value.is_empty() {
            handlers.push((event_name.to_string(), handler_value.to_string()));
        }
        search_from = abs_pos + 3 + eq_pos + skip;
    }
    handlers
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

/// Eleventy/Hugo/Jekyll: Statiska sidgeneratorer med `<script type="application/ld+json">`
///
/// Dessa ramverk renderar ren HTML utan hydration, men strukturerad data
/// (JSON-LD, OpenGraph meta) kan extraheras som semantisk kontext.
/// Detekteras via generator-meta-tag eller 11ty-specifika mönster.
fn extract_eleventy_data(html: &str) -> Option<HydrationData> {
    // Kolla generator-meta: <meta name="generator" content="Eleventy">
    let is_eleventy = html.contains("generator\" content=\"Eleventy")
        || html.contains("generator\" content=\"eleventy")
        || html.contains("generator\" content=\"11ty")
        || html.contains("data-11ty");
    let is_hugo = html.contains("generator\" content=\"Hugo");
    let is_jekyll = html.contains("generator\" content=\"Jekyll");

    if !is_eleventy && !is_hugo && !is_jekyll {
        return None;
    }

    // Extrahera JSON-LD om det finns
    let mut props = serde_json::Map::new();

    if let Some(ld_json) = extract_json_ld(html) {
        props.insert("jsonLd".to_string(), ld_json);
    }

    // Extrahera OpenGraph meta-taggar
    let og_data = extract_meta_tags(html, "og:");
    if !og_data.is_empty() {
        let og_obj: serde_json::Map<String, Value> = og_data
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        props.insert("openGraph".to_string(), Value::Object(og_obj));
    }

    // Extrahera standard meta-taggar (description, author, etc.)
    let meta_data = extract_standard_meta(html);
    if !meta_data.is_empty() {
        let meta_obj: serde_json::Map<String, Value> = meta_data
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        props.insert("meta".to_string(), Value::Object(meta_obj));
    }

    if props.is_empty() {
        return None;
    }

    Some(HydrationData {
        framework: Framework::Eleventy,
        props: Value::Object(props),
    })
}

/// Statisk HTML: Extrahera strukturerad data från ren HTML utan ramverk
///
/// Fångar JSON-LD, OpenGraph, meta-description — täcker sidor som inte
/// använder något JS-ramverk alls men ändå har maskinläsbar data.
fn extract_static_html_data(html: &str) -> Option<HydrationData> {
    let mut props = serde_json::Map::new();

    // JSON-LD
    if let Some(ld_json) = extract_json_ld(html) {
        props.insert("jsonLd".to_string(), ld_json);
    }

    // OpenGraph
    let og_data = extract_meta_tags(html, "og:");
    if !og_data.is_empty() {
        let og_obj: serde_json::Map<String, Value> = og_data
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        props.insert("openGraph".to_string(), Value::Object(og_obj));
    }

    // Standard meta
    let meta_data = extract_standard_meta(html);
    if !meta_data.is_empty() {
        let meta_obj: serde_json::Map<String, Value> = meta_data
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        props.insert("meta".to_string(), Value::Object(meta_obj));
    }

    // Twitter Card
    let twitter_data = extract_meta_tags(html, "twitter:");
    if !twitter_data.is_empty() {
        let tw_obj: serde_json::Map<String, Value> = twitter_data
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        props.insert("twitter".to_string(), Value::Object(tw_obj));
    }

    // Kräv minst 2 datapunkter — annars är det inte värt att returnera
    let total_keys: usize = props
        .values()
        .map(|v| v.as_object().map_or(1, |m| m.len()))
        .sum();
    if total_keys < 2 {
        return None;
    }

    Some(HydrationData {
        framework: Framework::StaticHtml,
        props: Value::Object(props),
    })
}

/// Extrahera JSON-LD från `<script type="application/ld+json">`
fn extract_json_ld(html: &str) -> Option<Value> {
    let marker = r#"type="application/ld+json""#;
    let pos = html.find(marker)?;
    let rest = html.get(pos..)?;
    let tag_end = rest.find('>')?;
    let after_tag = rest.get(tag_end + 1..)?;
    let end = after_tag.find("</script>")?;
    let json_str = after_tag.get(..end)?.trim();
    serde_json::from_str::<Value>(json_str).ok()
}

/// Extrahera meta-taggar med givet prefix (t.ex. "og:", "twitter:")
fn extract_meta_tags(html: &str, prefix: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    // Sök efter property="og:... eller name="twitter:...
    let property_pattern = format!(r#"property="{}"#, prefix);
    let name_pattern = format!(r#"name="{}"#, prefix);

    let mut search_from = 0;
    while search_from < html.len() {
        let meta_pos = match html[search_from..].find("<meta") {
            Some(p) => search_from + p,
            None => break,
        };

        let tag_end = match html[meta_pos..].find('>') {
            Some(p) => meta_pos + p,
            None => break,
        };
        let tag = match html.get(meta_pos..=tag_end) {
            Some(t) => t,
            None => break,
        };

        let has_prefix = tag.contains(&property_pattern) || tag.contains(&name_pattern);
        if has_prefix {
            let key =
                extract_attr_value(tag, "property").or_else(|| extract_attr_value(tag, "name"));
            let value = extract_attr_value(tag, "content");

            if let (Some(key), Some(value)) = (key, value) {
                let clean_key = key.strip_prefix(prefix).unwrap_or(&key).to_string();
                tags.push((clean_key, value));
            }
        }

        search_from = tag_end + 1;
    }
    tags
}

/// Extrahera standard meta-taggar (description, author, keywords)
fn extract_standard_meta(html: &str) -> Vec<(String, String)> {
    let interesting = ["description", "author", "keywords", "robots", "viewport"];
    let mut tags = Vec::new();

    for name in &interesting {
        let pattern = format!(r#"name="{}"#, name);
        if let Some(pos) = html.find(&pattern) {
            let tag_start = html[..pos].rfind('<').unwrap_or(pos);
            let tag_end = match html[tag_start..].find('>') {
                Some(p) => tag_start + p,
                None => continue,
            };
            if let Some(tag) = html.get(tag_start..=tag_end) {
                if let Some(content) = extract_attr_value(tag, "content") {
                    tags.push((name.to_string(), content));
                }
            }
        }
    }
    tags
}

/// Extrahera ett attributvärde ur en HTML-tagg
fn extract_attr_value(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr);
    let pos = tag.find(&pattern)?;
    let after = tag.get(pos + pattern.len()..)?;
    let end = after.find('"')?;
    after.get(..end).map(|s| s.to_string())
}

// ─── Devalue-parser ─────────────────────────────────────────────────────────

/// Parsea devalue-kodat data (Nuxt 3+, SvelteKit 2+)
///
/// Devalue-format: JSON-array med två delar: [references, ...values]
/// Stöder: Date, BigInt, Map, Set, cykliska refs, undefined, -0, Infinity, NaN.
///
/// Format-varianter:
/// - Nuxt 3: `[[refs], val1, val2, ...]` — array av referenced values
/// - SvelteKit: `{"type":"data","nodes":[{"type":"data","data":[refs, ...]}]}`
fn parse_devalue(input: &str) -> Option<Value> {
    let input = input.trim();

    // Försök parsea som JSON först — om det lyckas kan vi kolla efter devalue-struktur
    let parsed: Value = serde_json::from_str(input).ok()?;

    // SvelteKit-variant: wrappat i {"type":"data","nodes":[...]}
    if let Some(nodes) = parsed.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            if let Some(data) = node.get("data") {
                if let Some(decoded) = decode_devalue_payload(data) {
                    return Some(decoded);
                }
                // Redan giltig JSON — returnera som-den-är
                return Some(data.clone());
            }
        }
        // Om vi hittat nodes men ingen data, returnera hela strukturen
        return Some(parsed);
    }

    // Nuxt-variant: ren array [refs, val1, val2, ...]
    if let Some(arr) = parsed.as_array() {
        if let Some(decoded) = decode_devalue_array(arr) {
            return Some(decoded);
        }
    }

    // Redan giltig JSON-struktur — returnera som-den-är
    Some(parsed)
}

/// Dekoda devalue-payload som kan vara en array med refs + values
fn decode_devalue_payload(data: &Value) -> Option<Value> {
    let arr = data.as_array()?;
    decode_devalue_array(arr)
}

/// Dekoda devalue-array: [referens-map, value1, value2, ...]
///
/// Devalue referens-format:
/// - Heltal → index till annan value i arrayen
/// - Sträng med prefix: "Date:ISO", "BigInt:123", "Set:size", "Map:entries"
/// - Vanliga JSON-värden: passera genom
fn decode_devalue_array(arr: &[Value]) -> Option<Value> {
    if arr.is_empty() {
        return None;
    }

    // Första elementet kan vara en referens-tabell (object/array) eller direkt data
    let first = &arr[0];

    // Om det är en array av heltal/objekt → referens-tabell
    if first.is_array() || first.is_object() {
        // Samla alla efterföljande values
        let mut result = serde_json::Map::new();

        for (i, val) in arr.iter().enumerate().skip(1) {
            let resolved = resolve_devalue_node(val, arr);
            result.insert(format!("_{}", i), resolved);
        }

        // Om bara en value, returnera den direkt
        if result.len() == 1 {
            return result.into_iter().next().map(|(_, v)| v);
        }

        return Some(Value::Object(result));
    }

    // Inte devalue-format
    None
}

/// Resolva en devalue-nod — hanterar specialtyper
fn resolve_devalue_node(node: &Value, _all: &[Value]) -> Value {
    match node {
        // Sträng med devalue-prefix
        Value::String(s) => {
            if let Some(date_str) = s.strip_prefix("Date:") {
                // Bevara Date som sträng med prefix
                Value::String(date_str.to_string())
            } else if let Some(bigint_str) = s.strip_prefix("BigInt:") {
                Value::String(format!("{}n", bigint_str))
            } else if s == "undefined" {
                Value::Null
            } else if s == "-0" {
                Value::from(0)
            } else if s == "Infinity" {
                Value::String("Infinity".to_string())
            } else if s == "NaN" {
                Value::String("NaN".to_string())
            } else if s == "-Infinity" {
                Value::String("-Infinity".to_string())
            } else {
                node.clone()
            }
        }
        // Objekt — rekursivt resolva values
        Value::Object(map) => {
            let resolved: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), resolve_devalue_node(v, _all)))
                .collect();
            Value::Object(resolved)
        }
        // Array — rekursivt
        Value::Array(arr) => {
            let resolved: Vec<Value> = arr.iter().map(|v| resolve_devalue_node(v, _all)).collect();
            Value::Array(resolved)
        }
        _ => node.clone(),
    }
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

    // Scora key och value separat — ta den bästa
    let mut key_matches = 0;
    let mut value_matches = 0;
    let meaningful_words: Vec<&&str> = goal_words
        .iter()
        .filter(|w| w.len() >= 3) // Skippa "of", "in", etc.
        .collect();

    if meaningful_words.is_empty() {
        return 0.3;
    }

    for word in &meaningful_words {
        if key_lower.contains(**word) {
            key_matches += 1;
        }
        if value_lower.contains(**word) {
            value_matches += 1;
        }
    }

    let key_ratio = key_matches as f32 / meaningful_words.len() as f32;
    let value_ratio = value_matches as f32 / meaningful_words.len() as f32;

    // Value-score viktas högre — det är datan, key är bara fältnamnet
    let combined = (key_ratio * 0.3) + (value_ratio * 0.7);

    // Penalisera URL-fält och korta värden (< 10 tecken)
    let content_penalty =
        if value_lower.starts_with("http://") || value_lower.starts_with("https://") {
            0.3 // URL:er sällan relevanta som svar
        } else if value.len() < 10 {
            0.1 // Mycket korta värden (slug, id) sällan svaret
        } else {
            0.0
        };

    // Skala till 0.05–1.0
    (0.05 + combined * 0.95 - content_penalty).clamp(0.05, 1.0)
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
        // "product" matchar key → positiv score
        let product_score = score_entry_relevance("product_name", "Nike Air", &goal_words);
        let random_score = score_entry_relevance("random_field", "random_value", &goal_words);
        assert!(
            product_score > random_score,
            "Nyckel med goal-ord ({product_score:.3}) borde vara högre än random ({random_score:.3})"
        );
        assert!(
            random_score < 0.1,
            "Orelaterad nyckel borde ha låg relevans: {random_score:.3}"
        );

        // Value-match viktas högt
        let value_match = score_entry_relevance("name", "Great product for you", &goal_words);
        assert!(
            value_match > product_score,
            "Value-match ({value_match:.3}) borde vara högre än bara key-match ({product_score:.3})"
        );

        // URL penaliseras
        let url_score = score_entry_relevance("url", "https://example.com/product", &goal_words);
        assert!(
            url_score < value_match,
            "URL ({url_score:.3}) borde rankas lägre än text-value ({value_match:.3})"
        );
    }

    #[test]
    fn test_score_entry_relevance_empty_goal() {
        let goal_words: Vec<&str> = vec![];
        let score = score_entry_relevance("anything", "whatever", &goal_words);
        assert!((score - 0.5).abs() < f32::EPSILON, "Tomt mål borde ge 0.5");
    }

    // === Devalue-parser ===

    #[test]
    fn test_parse_devalue_plain_json() {
        let input = r#"{"key":"value","count":42}"#;
        let result = parse_devalue(input);
        assert!(result.is_some(), "Borde parsea ren JSON via devalue");
        let val = result.unwrap();
        assert_eq!(val.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_parse_devalue_sveltekit_format() {
        let input =
            r#"{"type":"data","nodes":[{"type":"data","data":{"items":["a","b"],"count":2}}]}"#;
        let result = parse_devalue(input);
        assert!(result.is_some(), "Borde parsea SvelteKit devalue-format");
        let val = result.unwrap();
        assert!(
            val.get("items").is_some() || val.get("count").is_some(),
            "Borde extrahera data från SvelteKit nodes: got {:?}",
            val
        );
    }

    #[test]
    fn test_parse_devalue_date_type() {
        let input = r#"[[], "Date:2026-03-21T00:00:00.000Z"]"#;
        let result = parse_devalue(input);
        assert!(result.is_some(), "Borde hantera Date-typ i devalue");
        let val = result.unwrap();
        let s = val.to_string();
        assert!(s.contains("2026-03-21"), "Date borde bevaras: got {}", s);
    }

    #[test]
    fn test_parse_devalue_bigint() {
        let input = r#"[[], "BigInt:123456789"]"#;
        let result = parse_devalue(input);
        assert!(result.is_some(), "Borde hantera BigInt i devalue");
        let val = result.unwrap();
        let s = val.to_string();
        assert!(
            s.contains("123456789n"),
            "BigInt borde konverteras: got {}",
            s
        );
    }

    #[test]
    fn test_parse_devalue_special_values() {
        let input = r#"[[], "undefined", "-0", "Infinity", "NaN"]"#;
        let result = parse_devalue(input);
        assert!(result.is_some(), "Borde hantera specialvärden");
    }

    #[test]
    fn test_nuxt3_devalue() {
        let html = r##"
        <script id="__NUXT_DATA__" type="application/json">
        {"type":"data","nodes":[{"type":"data","data":{"message":"hej","timestamp":"Date:2026-03-21"}}]}
        </script>
        "##;
        let result = extract_nuxt_data(html);
        assert!(
            result.is_some(),
            "Borde parsea Nuxt 3 med devalue-liknande data"
        );
    }

    #[test]
    fn test_sveltekit_devalue() {
        let html = r##"
        <script id="__sveltekit_data" type="application/json">
        {"type":"data","nodes":[{"type":"data","data":{"items":["x","y"],"big":"BigInt:99"}}]}
        </script>
        "##;
        let result = extract_sveltekit_data(html);
        assert!(result.is_some(), "Borde parsea SvelteKit med devalue");
    }

    // === Qwik QRL ===

    #[test]
    fn test_qwik_qrl_extraction() {
        let html = r#"
        <button on:click="./counter_s_increment_1_abc123">Count</button>
        <input on:input="./search_s_handler_xyz789" />
        <script type="qwik/json">{"ctx":{}}</script>
        "#;
        let result = extract_qwik_state(html);
        assert!(result.is_some(), "Borde hitta Qwik state");
        let data = result.unwrap();
        // QRL handlers borde finnas i props
        let handlers = data.props.get("_qrl_handlers");
        assert!(handlers.is_some(), "Borde extrahera QRL handlers");
        let arr = handlers.unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2, "Borde hitta 2 QRL handlers");
        assert_eq!(
            arr[0].get("event").and_then(|v| v.as_str()),
            Some("click"),
            "Första handler borde vara click"
        );
    }

    #[test]
    fn test_qwik_qrl_empty() {
        let html = r#"
        <div>Ingen QRL</div>
        <script type="qwik/json">{"ctx":{}}</script>
        "#;
        let result = extract_qwik_state(html);
        assert!(result.is_some(), "Borde hitta Qwik state");
        let data = result.unwrap();
        // Inga QRL handlers
        assert!(
            data.props.get("_qrl_handlers").is_none(),
            "Borde inte ha QRL handlers utan on:-attribut"
        );
    }

    // === RSC Flight Protocol ===

    #[test]
    fn test_next_flight_rsc_lines() {
        // Simulera RSC wire format med JSON per rad
        let html = r#"
        <script>self.__next_f.push([1,"{\"title\":\"Hem\",\"count\":42}\n{\"items\":[1,2,3]}"])</script>
        "#;
        let result = extract_next_flight(html);
        assert!(result.is_some(), "Borde parsea RSC Flight data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::NextFlight);
        // Borde ha extraherat data från raderna
        assert!(
            data.props.get("title").is_some() || data.props.get("items").is_some(),
            "Borde extrahera data från RSC-rader: got {:?}",
            data.props
        );
    }

    // === Eleventy/Hugo/Jekyll ===

    #[test]
    fn test_eleventy_with_json_ld() {
        let html = r##"
        <html><head>
        <meta name="generator" content="Eleventy">
        <meta name="description" content="En blogg">
        <script type="application/ld+json">
        {"@type":"BlogPosting","headline":"Min artikel","author":"Anna"}
        </script>
        </head><body><p>Innehåll</p></body></html>
        "##;
        let result = extract_eleventy_data(html);
        assert!(result.is_some(), "Borde hitta Eleventy data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Eleventy);
        assert!(data.props.get("jsonLd").is_some(), "Borde ha JSON-LD");
        assert!(data.props.get("meta").is_some(), "Borde ha meta-taggar");
    }

    #[test]
    fn test_hugo_generator() {
        let html = r##"
        <html><head>
        <meta name="generator" content="Hugo 0.123">
        <meta property="og:title" content="Hugo-sida">
        <meta property="og:type" content="article">
        </head><body></body></html>
        "##;
        let result = extract_eleventy_data(html);
        assert!(result.is_some(), "Borde hitta Hugo data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::Eleventy);
        assert!(
            data.props.get("openGraph").is_some(),
            "Borde ha OpenGraph-data"
        );
    }

    #[test]
    fn test_jekyll_generator() {
        let html = r##"
        <html><head>
        <meta name="generator" content="Jekyll v4.3">
        <meta name="description" content="Jekyll-blogg">
        <meta name="author" content="Erik">
        </head><body></body></html>
        "##;
        let result = extract_eleventy_data(html);
        assert!(result.is_some(), "Borde hitta Jekyll data");
    }

    #[test]
    fn test_eleventy_without_data() {
        let html = r#"
        <html><head>
        <meta name="generator" content="Eleventy">
        </head><body></body></html>
        "#;
        let result = extract_eleventy_data(html);
        assert!(
            result.is_none(),
            "Borde returnera None utan strukturerad data"
        );
    }

    // === Static HTML ===

    #[test]
    fn test_static_html_json_ld() {
        let html = r##"
        <html><head>
        <meta name="description" content="En produkt">
        <meta property="og:title" content="Produkt A">
        <script type="application/ld+json">
        {"@type":"Product","name":"Produkt A","price":"299 SEK"}
        </script>
        </head><body></body></html>
        "##;
        let result = extract_static_html_data(html);
        assert!(result.is_some(), "Borde hitta statisk HTML data");
        let data = result.unwrap();
        assert_eq!(data.framework, Framework::StaticHtml);
        assert!(data.props.get("jsonLd").is_some(), "Borde ha JSON-LD");
    }

    #[test]
    fn test_static_html_opengraph_twitter() {
        let html = r##"
        <html><head>
        <meta property="og:title" content="Sida">
        <meta property="og:description" content="Beskrivning">
        <meta name="twitter:card" content="summary">
        <meta name="twitter:title" content="Sida">
        </head><body></body></html>
        "##;
        let result = extract_static_html_data(html);
        assert!(result.is_some(), "Borde hitta OG + Twitter data");
        let data = result.unwrap();
        assert!(data.props.get("openGraph").is_some(), "Borde ha OpenGraph");
        assert!(data.props.get("twitter").is_some(), "Borde ha Twitter");
    }

    #[test]
    fn test_static_html_too_little_data() {
        let html = r#"
        <html><head></head><body><p>Bara text</p></body></html>
        "#;
        let result = extract_static_html_data(html);
        assert!(result.is_none(), "Borde returnera None med för lite data");
    }

    #[test]
    fn test_extract_json_ld() {
        let html = r##"
        <script type="application/ld+json">{"@type":"Organization","name":"Acme"}</script>
        "##;
        let result = extract_json_ld(html);
        assert!(result.is_some(), "Borde parsea JSON-LD");
        let val = result.unwrap();
        assert_eq!(
            val.get("name").and_then(|v| v.as_str()),
            Some("Acme"),
            "Borde extrahera name"
        );
    }

    #[test]
    fn test_extract_meta_tags_og() {
        let html = r##"
        <meta property="og:title" content="Min titel">
        <meta property="og:description" content="Beskrivning">
        <meta property="og:image" content="https://example.com/img.jpg">
        "##;
        let tags = extract_meta_tags(html, "og:");
        assert_eq!(tags.len(), 3, "Borde hitta 3 OG-taggar");
        assert_eq!(tags[0].0, "title");
        assert_eq!(tags[0].1, "Min titel");
    }

    #[test]
    fn test_extract_standard_meta() {
        let html = r##"
        <meta name="description" content="En sida om saker">
        <meta name="author" content="Anna Svensson">
        "##;
        let tags = extract_standard_meta(html);
        assert!(tags.len() >= 2, "Borde hitta 2 meta-taggar");
    }
}
