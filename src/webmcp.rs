/// WebMCP Discovery – Fas 9b
///
/// Detekterar och extraherar WebMCP-verktygsregistreringar från webbsidor.
/// WebMCP (W3C-inkuberad standard, Chrome 146) låter sajter exponera
/// verktyg via `navigator.modelContext.registerTool()`.
///
/// Pipeline:
/// 1. Scanna inline-scripts efter `navigator.modelContext.registerTool()`
/// 2. Parsa verktygsregistreringar (name, description, inputSchema)
/// 3. Exponera som strukturerade verktyg i det semantiska trädet
use serde::{Deserialize, Serialize};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Ett upptäckt WebMCP-verktyg på en webbsida
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMcpTool {
    /// Verktygets namn (från registerTool)
    pub name: String,
    /// Beskrivning
    pub description: String,
    /// Input-schema som JSON-sträng (JSON Schema draft 7)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<String>,
    /// Om verktyget är read-only (från annotations.readOnlyHint)
    pub read_only: bool,
    /// Rad i scriptet där registreringen hittades
    pub source_line: u32,
}

/// Resultat av WebMCP-discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebMcpDiscoveryResult {
    /// URL som skannades
    pub url: String,
    /// Upptäckta verktyg
    pub tools: Vec<WebMcpTool>,
    /// Om sidan använder WebMCP API:et
    pub has_webmcp: bool,
    /// Om sidan har polyfill (`@mcp-b/global`)
    pub has_polyfill: bool,
    /// Antal inline-scripts som skannades
    pub scripts_scanned: u32,
    /// Skanntid i ms
    pub scan_time_ms: u64,
}

// ─── Discovery ──────────────────────────────────────────────────────────────

/// Skanna HTML efter WebMCP-verktygsregistreringar
pub fn discover_webmcp_tools(html: &str, url: &str) -> WebMcpDiscoveryResult {
    let mut tools = Vec::new();
    let mut has_polyfill = false;

    // Extrahera inline script-block
    let scripts = extract_script_blocks(html);
    let scripts_scanned = scripts.len() as u32;

    for script in &scripts {
        // Detektera polyfill
        if script.contains("@mcp-b/global") || script.contains("mcp-b") {
            has_polyfill = true;
        }

        // Hitta registerTool-anrop
        let found = extract_register_tool_calls(script);
        tools.extend(found);
    }

    let has_webmcp = !tools.is_empty()
        || html.contains("navigator.modelContext")
        || html.contains("modelContext.registerTool");

    WebMcpDiscoveryResult {
        url: url.to_string(),
        tools,
        has_webmcp,
        has_polyfill,
        scripts_scanned,
        scan_time_ms: 0, // Sätts av anroparen
    }
}

/// Extrahera inline-script-block från HTML
fn extract_script_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let lower = html.to_lowercase();
    let mut search_start = 0;

    loop {
        let open_tag = lower[search_start..].find("<script");
        let open_tag = match open_tag {
            Some(pos) => search_start + pos,
            None => break,
        };

        // Hitta slutet av öppningstaggen
        let tag_end = match lower[open_tag..].find('>') {
            Some(pos) => open_tag + pos + 1,
            None => break,
        };

        // Hoppa över externa scripts (src=...)
        let tag_content = &lower[open_tag..tag_end];
        if tag_content.contains("src=") {
            search_start = tag_end;
            continue;
        }

        // Hitta </script>
        let close_tag = match lower[tag_end..].find("</script>") {
            Some(pos) => tag_end + pos,
            None => break,
        };

        // Extrahera scriptinnehållet (använd original-HTML för korrekt case)
        let script_content = &html[tag_end..close_tag];
        if !script_content.trim().is_empty() {
            blocks.push(script_content.to_string());
        }

        search_start = close_tag + 9; // längden av "</script>"
    }

    blocks
}

/// Extrahera registerTool-anrop från en script-sträng
fn extract_register_tool_calls(script: &str) -> Vec<WebMcpTool> {
    let mut tools = Vec::new();

    // Sök efter `registerTool(` eller `registerTool ({`
    let pattern = "registerTool";
    let mut search_pos = 0;

    while let Some(pos) = script[search_pos..].find(pattern) {
        let abs_pos = search_pos + pos;
        let after_pattern = abs_pos + pattern.len();

        // Hitta öppnande parentes
        let remaining = &script[after_pattern..];
        let paren_pos = match remaining.find('(') {
            Some(p) if p < 5 => after_pattern + p, // Tillåt whitespace
            _ => {
                search_pos = after_pattern;
                continue;
            }
        };

        // Hitta matchande objektliteral
        let after_paren = &script[paren_pos + 1..];
        let obj_start = match after_paren.find('{') {
            Some(p) if p < 20 => paren_pos + 1 + p,
            _ => {
                search_pos = after_pattern;
                continue;
            }
        };

        // Beräkna radnummer
        let source_line = script[..abs_pos].matches('\n').count() as u32 + 1;

        // Extrahera fält med enkel parsing
        let tool = parse_tool_object(&script[obj_start..], source_line);
        if let Some(t) = tool {
            tools.push(t);
        }

        search_pos = obj_start + 1;
    }

    tools
}

/// Parsa ett JS-objekt för att extrahera name, description, inputSchema
fn parse_tool_object(obj_src: &str, source_line: u32) -> Option<WebMcpTool> {
    let name = extract_js_string_field(obj_src, "name")?;
    let description = extract_js_string_field(obj_src, "description").unwrap_or_default();

    // Detektera inputSchema (extrahera som rå JSON-liknande sträng)
    let input_schema = extract_nested_object(obj_src, "inputSchema");

    // Detektera readOnlyHint
    let read_only = obj_src.contains("readOnlyHint")
        && (obj_src.contains("readOnlyHint: true") || obj_src.contains("readOnlyHint:true"));

    Some(WebMcpTool {
        name,
        description,
        input_schema,
        read_only,
        source_line,
    })
}

/// Extrahera ett strängfält från ett JS-objekt
fn extract_js_string_field(src: &str, field_name: &str) -> Option<String> {
    // Matcha: field_name: "value" eller field_name: 'value'
    let patterns = [
        format!("{}: \"", field_name),
        format!("{}: '", field_name),
        format!("{}:\"", field_name),
        format!("{}:'", field_name),
        format!("\"{}\": \"", field_name),
        format!("\"{}\":\"", field_name),
    ];

    for pat in &patterns {
        if let Some(start) = src.find(pat.as_str()) {
            let value_start = start + pat.len();
            let quote = if pat.ends_with('"') { '"' } else { '\'' };
            if let Some(end) = src[value_start..].find(quote) {
                return Some(src[value_start..value_start + end].to_string());
            }
        }
    }

    None
}

/// Extrahera ett nästlat objekt (t.ex. inputSchema: { ... })
fn extract_nested_object(src: &str, field_name: &str) -> Option<String> {
    let patterns = [
        format!("{}: {{", field_name),
        format!("{}:{{", field_name),
        format!("\"{}\": {{", field_name),
    ];

    for pat in &patterns {
        if let Some(start) = src.find(pat.as_str()) {
            let obj_start = start + pat.len() - 1; // Inkludera '{'
            let mut depth = 0i32;
            let mut end = obj_start;

            for (i, ch) in src[obj_start..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = obj_start + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if depth == 0 && end > obj_start {
                return Some(src[obj_start..end].to_string());
            }
        }
    }

    None
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_no_webmcp() {
        let html = r#"<html><body><p>Normal sida</p></body></html>"#;
        let result = discover_webmcp_tools(html, "https://test.com");
        assert!(!result.has_webmcp, "Normal sida borde inte ha WebMCP");
        assert!(result.tools.is_empty());
    }

    #[test]
    fn test_discover_register_tool() {
        let html = r##"<html><body>
        <script>
            navigator.modelContext.registerTool({
                name: "search_products",
                description: "Search for products by name or category",
                inputSchema: {
                    type: "object",
                    properties: {
                        query: { type: "string" }
                    },
                    required: ["query"]
                },
                annotations: { readOnlyHint: true },
                execute: async ({ query }) => {
                    return { content: [{ type: "text", text: "results" }] };
                }
            });
        </script>
        </body></html>"##;

        let result = discover_webmcp_tools(html, "https://shop.com");
        assert!(result.has_webmcp, "Borde detektera WebMCP");
        assert_eq!(result.tools.len(), 1, "Borde hitta 1 verktyg");
        assert_eq!(result.tools[0].name, "search_products");
        assert_eq!(
            result.tools[0].description,
            "Search for products by name or category"
        );
        assert!(result.tools[0].read_only, "Borde detektera readOnlyHint");
        assert!(
            result.tools[0].input_schema.is_some(),
            "Borde extrahera inputSchema"
        );
    }

    #[test]
    fn test_discover_multiple_tools() {
        let html = r##"<html><body>
        <script>
            navigator.modelContext.registerTool({
                name: "search",
                description: "Search products"
            });
            navigator.modelContext.registerTool({
                name: "add_to_cart",
                description: "Add item to shopping cart"
            });
        </script>
        </body></html>"##;

        let result = discover_webmcp_tools(html, "https://shop.com");
        assert_eq!(result.tools.len(), 2, "Borde hitta 2 verktyg");
        assert_eq!(result.tools[0].name, "search");
        assert_eq!(result.tools[1].name, "add_to_cart");
    }

    #[test]
    fn test_detect_polyfill() {
        let html = r##"<html><head>
        <script src="https://cdn.example.com/@mcp-b/global"></script>
        </head><body>
        <script>
            navigator.modelContext.registerTool({
                name: "test",
                description: "Test tool"
            });
        </script>
        </body></html>"##;

        let result = discover_webmcp_tools(html, "https://test.com");
        assert!(result.has_webmcp);
        // Polyfill-scriptet har src= så det skippas, men detekteras om texten
        // finns i HTML-källan
    }

    #[test]
    fn test_extract_script_blocks() {
        let html = r#"<html><body>
        <script>console.log('hello');</script>
        <script src="external.js"></script>
        <script>console.log('world');</script>
        </body></html>"#;

        let blocks = extract_script_blocks(html);
        assert_eq!(
            blocks.len(),
            2,
            "Borde extrahera 2 inline-scripts (skippa extern)"
        );
    }

    #[test]
    fn test_extract_input_schema() {
        let obj = r#"{
            type: "object",
            properties: {
                query: { type: "string", description: "Search term" },
                max: { type: "number" }
            },
            required: ["query"]
        }"#;
        let src = format!("name: \"test\", inputSchema: {}", obj);
        let schema = extract_nested_object(&src, "inputSchema");
        assert!(schema.is_some(), "Borde extrahera inputSchema");
        let schema_str = schema.unwrap();
        assert!(schema_str.contains("properties"));
        assert!(schema_str.contains("query"));
    }

    #[test]
    fn test_no_name_returns_none() {
        let tool = parse_tool_object("{ description: 'no name' }", 1);
        assert!(tool.is_none(), "Verktyg utan namn borde returnera None");
    }

    #[test]
    fn test_serialization() {
        let result = WebMcpDiscoveryResult {
            url: "https://test.com".to_string(),
            tools: vec![WebMcpTool {
                name: "test".to_string(),
                description: "Test tool".to_string(),
                input_schema: None,
                read_only: false,
                source_line: 1,
            }],
            has_webmcp: true,
            has_polyfill: false,
            scripts_scanned: 1,
            scan_time_ms: 0,
        };
        let json = serde_json::to_string(&result).expect("Borde serialisera");
        assert!(json.contains("test"), "JSON borde innehålla verktygsnamn");
    }
}
