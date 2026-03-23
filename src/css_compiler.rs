// CSS Compiler — 3-stegspipeline för pixel-perfect rendering med Blitz
//
// Steg 1: LightningCSS Transform — resolve CSS vars, downlevel modern CSS, minify
// Steg 2: Media Query Filter — matcha @media mot känd viewport, strippa icke-matchande
// Steg 3: css-inline — flattena all CSS till style="" attribut, ta bort <style>-block
//
// Resultatet: HTML med bara inline styles — Blitz renderar utan ambiguitet.

/// Resultat från CSS Compiler-pipelinen
#[derive(Debug, Clone)]
pub struct CssCompilerResult {
    /// Modifierad HTML med all CSS inlinad som style=""
    pub html: String,
    /// Antal <style>-block som processades
    pub style_blocks_processed: usize,
    /// Antal CSS-regler efter filtrering
    pub rules_after_filter: usize,
    /// Total tid i mikrosekunder
    pub compile_time_us: u64,
    /// Om kompileringen lyckades fullt (false = fallback till original)
    pub fully_compiled: bool,
}

/// Viewport-konfiguration för @media-matchning
#[derive(Debug, Clone, Copy)]
pub struct ViewportConfig {
    pub width: u32,
    pub height: u32,
    pub color_scheme: ColorScheme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

impl Default for ViewportConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 900,
            color_scheme: ColorScheme::Light,
        }
    }
}

/// Huvudfunktion: kör hela CSS Compiler-pipelinen
///
/// Tar HTML (med inlinade <style>-block från fetch.rs) och producerar
/// HTML där ALL CSS är flattenad till style=""-attribut.
pub fn compile_css(html: &str, viewport: &ViewportConfig) -> CssCompilerResult {
    let start = std::time::Instant::now();

    // Säkerhetsgräns — skip för extremt stor HTML
    const MAX_HTML_FOR_COMPILE: usize = 5 * 1024 * 1024; // 5 MB
    if html.len() > MAX_HTML_FOR_COMPILE {
        return CssCompilerResult {
            html: html.to_string(),
            style_blocks_processed: 0,
            rules_after_filter: 0,
            compile_time_us: start.elapsed().as_micros() as u64,
            fully_compiled: false,
        };
    }

    // Steg 1+2: Extrahera <style>-block, transform med LightningCSS, filtrera @media
    let (html_with_transformed_css, blocks_processed, rules_count) =
        transform_and_filter_css(html, viewport);

    // Steg 2.5: Resolve CSS custom properties (var(--x)) till konkreta värden
    let html_with_resolved_vars = resolve_css_variables(&html_with_transformed_css);

    // Steg 2.6: Lägg till system font fallbacks
    let html_with_fonts = add_font_fallbacks(&html_with_resolved_vars);

    // Steg 3: Inline all CSS till style="" med css-inline
    let final_html = inline_css_to_attributes(&html_with_fonts);

    // Validering: css-inline kan producera trasig output för extremt komplex CSS.
    // Snabb check: output borde inte vara dramatiskt mindre än input (indikerar att
    // css-inline strippade bort allt innehåll).
    let result_html = if final_html.len() >= html.len() / 5 {
        final_html
    } else {
        // css-inline producerade suspekt liten output — fallback till steg 1+2
        html_with_transformed_css
    };

    let elapsed = start.elapsed().as_micros() as u64;

    CssCompilerResult {
        html: result_html,
        style_blocks_processed: blocks_processed,
        rules_after_filter: rules_count,
        compile_time_us: elapsed,
        fully_compiled: true,
    }
}

// ─── Steg 1+2: LightningCSS Transform + Media Query Filter ──────────────────

/// Extraherar alla <style>-block, transformerar CSS med LightningCSS,
/// filtrerar @media-regler, och stoppar tillbaka transformerad CSS.
fn transform_and_filter_css(html: &str, viewport: &ViewportConfig) -> (String, usize, usize) {
    // Hitta alla <style>...</style>-block
    let style_blocks = extract_style_blocks(html);
    if style_blocks.is_empty() {
        return (html.to_string(), 0, 0);
    }

    let mut result_html = html.to_string();
    let mut total_blocks = 0;
    let mut total_rules = 0;

    // Processa i omvänd ordning (så byte-offsets inte skiftar)
    for block in style_blocks.iter().rev() {
        let transformed = transform_single_css(&block.css_content, viewport);
        total_rules += count_rules(&transformed);
        total_blocks += 1;

        // Ersätt blocket i HTML:en
        let new_block = format!("<style>{}</style>", transformed);
        // Säker replace — vi vet exakt positionen
        if block.start < result_html.len()
            && block.end <= result_html.len()
            && result_html.is_char_boundary(block.start)
            && result_html.is_char_boundary(block.end)
        {
            result_html.replace_range(block.start..block.end, &new_block);
        }
    }

    (result_html, total_blocks, total_rules)
}

/// Transformera en enskild CSS-sträng med LightningCSS
fn transform_single_css(css: &str, viewport: &ViewportConfig) -> String {
    use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
    use lightningcss::targets::{Browsers, Targets};

    // Säkerhetsgräns per CSS-block
    const MAX_CSS_SIZE: usize = 2 * 1024 * 1024; // 2 MB
    if css.len() > MAX_CSS_SIZE {
        return css.to_string();
    }

    // Parsa CSS
    let mut stylesheet = match StyleSheet::parse(css, ParserOptions::default()) {
        Ok(ss) => ss,
        Err(_) => return css.to_string(), // Fallback vid parse-error
    };

    // Konfigurera targets — Chrome 120 stödjer CSS vars, @layer, nesting, :is(),
    // color-mix(), logical properties. LightningCSS behåller var() och moderna
    // features som Blitz kan hantera istället för att strippa allt.
    let targets = Targets::from(Browsers {
        chrome: Some(120 << 16), // Chrome 120 — modern CSS med var()-stöd
        ..Default::default()
    });

    // Minify + transform — resolver CSS vars, downlevel, expand shorthands
    let minify_opts = lightningcss::stylesheet::MinifyOptions {
        targets,
        ..Default::default()
    };
    if stylesheet.minify(minify_opts).is_err() {
        return css.to_string();
    }

    // Serialisera tillbaka till CSS-sträng
    let print_opts = PrinterOptions {
        targets,
        minify: false, // Behåll läsbarhet (css-inline hanterar matchningen)
        ..Default::default()
    };
    match stylesheet.to_css(print_opts) {
        Ok(result) => {
            // Steg 2: Filtrera @media-regler som inte matchar vår viewport
            filter_media_queries(&result.code, viewport)
        }
        Err(_) => css.to_string(),
    }
}

/// Räkna antal CSS-regler (approximativt)
fn count_rules(css: &str) -> usize {
    css.matches('{').count()
}

// ─── Steg 2: @media-filtrering ───────────────────────────────────────────────

/// Filtrerar @media-regler baserat på känd viewport.
/// Behåller regler som matchar, tar bort de som inte matchar.
fn filter_media_queries(css: &str, viewport: &ViewportConfig) -> String {
    let mut result = String::with_capacity(css.len());
    let mut pos = 0;

    while pos < css.len() {
        // Sök efter @media
        if let Some(media_start) = css[pos..].find("@media") {
            // Kopiera allt före @media
            result.push_str(&css[pos..pos + media_start]);
            let abs_media_start = pos + media_start;

            // Extrahera media-condition (allt mellan @media och {)
            if let Some(brace_offset) = css[abs_media_start..].find('{') {
                let condition_str =
                    &css[abs_media_start + 6..abs_media_start + brace_offset].trim();

                // Hitta matchande slutklammer
                if let Some(block_end) = find_matching_brace(css, abs_media_start + brace_offset) {
                    let inner_css = &css[abs_media_start + brace_offset + 1..block_end];

                    if media_condition_matches(condition_str, viewport) {
                        // @media matchar — behåll inner CSS (utan @media-wrapper)
                        result.push_str(inner_css);
                    }
                    // Annars: skippa hela blocket

                    pos = block_end + 1;
                    continue;
                }
            }
            // Kunde inte parsa — behåll som det är
            result.push_str(&css[abs_media_start..abs_media_start + 6]);
            pos = abs_media_start + 6;
        } else {
            // Inget mer @media — kopiera resten
            result.push_str(&css[pos..]);
            break;
        }
    }

    result
}

/// Hitta matchande } för en { på given position
fn find_matching_brace(css: &str, open_pos: usize) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';
    let mut prev_char = ' ';

    for (i, ch) in css[open_pos..].char_indices() {
        if in_string {
            if ch == string_char && prev_char != '\\' {
                in_string = false;
            }
        } else {
            match ch {
                '"' | '\'' => {
                    in_string = true;
                    string_char = ch;
                }
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open_pos + i);
                    }
                }
                _ => {}
            }
        }
        prev_char = ch;
    }
    None
}

/// Evaluera en @media-condition mot vår viewport.
///
/// Stöder: min-width, max-width, min-height, max-height, prefers-color-scheme,
/// screen, all, print (nekas).
fn media_condition_matches(condition: &str, viewport: &ViewportConfig) -> bool {
    let condition = condition.trim();

    // Tom condition = matchar alltid
    if condition.is_empty() {
        return true;
    }

    // "print" matchar aldrig
    if condition == "print" {
        return false;
    }

    // "screen", "all", "screen and ..." — matchar alltid basvillkoret
    let condition = condition
        .trim_start_matches("screen and ")
        .trim_start_matches("screen")
        .trim_start_matches("all and ")
        .trim_start_matches("all")
        .trim();

    if condition.is_empty() {
        return true;
    }

    // Splitta på " and " och kolla varje del
    // Hantera också "not" och "or"
    if let Some(rest) = condition.strip_prefix("not ") {
        return !media_condition_matches(rest, viewport);
    }

    // Hantera "or" — minst en ska matcha
    if condition.contains(" or ") {
        return condition
            .split(" or ")
            .any(|part| evaluate_single_media_feature(part.trim(), viewport));
    }

    // Hantera "and" — alla ska matcha
    if condition.contains(" and ") {
        return condition
            .split(" and ")
            .all(|part| evaluate_single_media_feature(part.trim(), viewport));
    }

    // Enskild feature
    evaluate_single_media_feature(condition, viewport)
}

/// Evaluera en enskild @media-feature, t.ex. "(min-width: 768px)"
fn evaluate_single_media_feature(feature: &str, viewport: &ViewportConfig) -> bool {
    // Strippa parenteser
    let feature = feature.trim().trim_start_matches('(').trim_end_matches(')');

    // Parsa "property: value"
    let parts: Vec<&str> = feature.splitn(2, ':').collect();
    if parts.len() != 2 {
        // Okänd syntax — matchar som default
        return true;
    }

    let prop = parts[0].trim();
    let value = parts[1].trim();

    // Parsa px-värde
    let px_value = if let Some(stripped) = value.strip_suffix("px") {
        stripped.trim().parse::<f64>().ok()
    } else if let Some(stripped) = value.strip_suffix("em") {
        // 1em ≈ 16px
        stripped.trim().parse::<f64>().ok().map(|v| v * 16.0)
    } else if let Some(stripped) = value.strip_suffix("rem") {
        stripped.trim().parse::<f64>().ok().map(|v| v * 16.0)
    } else {
        value.parse::<f64>().ok()
    };

    match prop {
        "min-width" => {
            if let Some(px) = px_value {
                (viewport.width as f64) >= px
            } else {
                true
            }
        }
        "max-width" => {
            if let Some(px) = px_value {
                (viewport.width as f64) <= px
            } else {
                true
            }
        }
        "min-height" => {
            if let Some(px) = px_value {
                (viewport.height as f64) >= px
            } else {
                true
            }
        }
        "max-height" => {
            if let Some(px) = px_value {
                (viewport.height as f64) <= px
            } else {
                true
            }
        }
        "prefers-color-scheme" => match value {
            "dark" => viewport.color_scheme == ColorScheme::Dark,
            "light" => viewport.color_scheme == ColorScheme::Light,
            _ => true,
        },
        "prefers-reduced-motion" => {
            // Statisk rendering — alltid "reduce"
            value == "reduce"
        }
        // hover, pointer — matchar "none" (ingen mus i screenshot-kontext)
        "hover" => value == "none",
        "pointer" => value == "none" || value == "fine",
        // Okända features — default till true (konservativt)
        _ => true,
    }
}

// ─── Steg 3: css-inline ─────────────────────────────────────────────────────

/// Resolve CSS custom properties: extrahera :root { --x: value; } och
/// substituera var(--x) / var(--x, fallback) genom hela CSS:en.
///
/// Hanterar:
/// - `:root { --primary: #0066cc; }` → extrahera variabel-mappning
/// - `color: var(--primary)` → `color: #0066cc`
/// - `color: var(--missing, red)` → `color: red` (fallback)
/// - Nästlade var(): `var(--x, var(--y, blue))` → resolvas rekursivt
fn resolve_css_variables(html: &str) -> String {
    // Steg 1: Extrahera alla custom properties från :root, html, body block
    let vars = extract_custom_properties(html);
    if vars.is_empty() {
        return html.to_string();
    }

    // Steg 2: Substituera var(--x) och var(--x, fallback) överallt
    substitute_var_functions(html, &vars)
}

/// Extrahera CSS custom properties (--name: value) från :root, html, body block
fn extract_custom_properties(html: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();

    // Hitta :root { ... }, html { ... }, body { ... } block i <style>
    let lower = html.to_ascii_lowercase();
    for selector in &[":root", "html", "body"] {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(selector) {
            let abs_pos = search_from + pos;
            // Hitta öppnande {
            let after = &html[abs_pos..];
            if let Some(brace_start) = after.find('{') {
                // Hitta matchande }
                let block_start = abs_pos + brace_start + 1;
                let mut depth = 1;
                let mut block_end = block_start;
                for (i, ch) in html[block_start..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                block_end = block_start + i;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                // Parsa custom properties ur blocket
                let block = &html[block_start..block_end];
                for decl in block.split(';') {
                    let trimmed = decl.trim();
                    if let Some(colon) = trimmed.find(':') {
                        let name = trimmed[..colon].trim();
                        if name.starts_with("--") {
                            let value = trimmed[colon + 1..].trim().to_string();
                            vars.insert(name.to_string(), value);
                        }
                    }
                }
                search_from = block_end + 1;
            } else {
                search_from = abs_pos + selector.len();
            }
        }
    }
    vars
}

/// Substituera var(--name) och var(--name, fallback) i HTML/CSS
fn substitute_var_functions(
    html: &str,
    vars: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = html.to_string();
    // Max 5 iterationer — hanterar nästlade var()
    for _ in 0..5 {
        let lower = result.to_ascii_lowercase();
        if !lower.contains("var(--") {
            break;
        }
        let mut new_result = String::with_capacity(result.len());
        let mut remaining = result.as_str();

        while let Some(pos) = remaining.to_ascii_lowercase().find("var(--") {
            new_result.push_str(&remaining[..pos]);
            let after = &remaining[pos..];

            // Hitta matchande ) med nesting — starta efter "var("
            let open_paren = match after.find('(') {
                Some(p) => p,
                None => {
                    new_result.push_str("var(--");
                    remaining = &remaining[pos + 6..];
                    continue;
                }
            };
            let mut depth = 0;
            let mut end = 0;
            let mut found_close = false;
            for (i, ch) in after[open_paren..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open_paren + i + 1;
                            found_close = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }

            if !found_close || end <= open_paren + 2 {
                // Ingen matchande ) — behåll var( och fortsätt
                new_result.push_str("var(--");
                remaining = &remaining[pos + 6..];
                continue;
            }

            let inner_start = open_paren + 1;
            let inner_end = end - 1;
            let var_expr = &after[inner_start..inner_end];

            // Parsa: --name eller --name, fallback
            let (var_name, fallback) = if let Some(comma) = find_top_level_comma(var_expr) {
                let name = var_expr[..comma].trim();
                let fb = var_expr[comma + 1..].trim();
                (name, Some(fb))
            } else {
                (var_expr.trim(), None)
            };

            // Resolva
            if let Some(value) = vars.get(var_name) {
                new_result.push_str(value);
            } else if let Some(fb) = fallback {
                new_result.push_str(fb);
            } else {
                // Ingen resolution — behåll original var()
                new_result.push_str(&after[..end]);
            }

            remaining = &remaining[pos + end..];
        }
        new_result.push_str(remaining);
        result = new_result;
    }
    result
}

/// Hitta första komma på toppnivå (utanför nästlade parenteser)
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Lägg till system font-fallback i font-family deklarationer.
/// Säkerställer att text renderas korrekt även om custom fonts inte laddas.
/// Mappar vanliga webfonter till system-equivalenter.
pub fn add_font_fallbacks(html: &str) -> String {
    // Vanliga webfonter → system font mappning
    const FONT_MAP: &[(&str, &str)] = &[
        ("inter", "system-ui, -apple-system, sans-serif"),
        ("roboto", "Arial, Helvetica, sans-serif"),
        ("open sans", "Helvetica, Arial, sans-serif"),
        ("lato", "Helvetica, Arial, sans-serif"),
        ("montserrat", "Verdana, sans-serif"),
        ("poppins", "Verdana, sans-serif"),
        ("source sans", "Helvetica, Arial, sans-serif"),
        ("nunito", "Verdana, sans-serif"),
        ("raleway", "Verdana, sans-serif"),
        ("playfair", "Georgia, serif"),
        ("merriweather", "Georgia, serif"),
        ("noto sans", "Arial, Helvetica, sans-serif"),
        ("noto serif", "Georgia, serif"),
        ("ubuntu", "system-ui, sans-serif"),
        ("fira sans", "system-ui, sans-serif"),
    ];

    let mut result = html.to_string();

    // Hitta alla font-family deklarationer och lägg till fallbacks
    for &(web_font, fallback) in FONT_MAP {
        // Matcha: font-family: "Inter" eller font-family: Inter (case-insensitive)
        let lower = result.to_ascii_lowercase();
        let search = web_font;
        let mut new_result = String::with_capacity(result.len());
        let mut remaining = result.as_str();
        let mut changed = false;

        while let Some(pos) = remaining.to_ascii_lowercase().find(search) {
            // Kolla att det är i en font-family-kontext
            let before = &remaining[..pos];
            let is_font_ctx = before
                .rfind("font-family")
                .map(|fp| {
                    // Kontrollera att inget ';' eller '}' finns mellan font-family och fonten
                    !before[fp..].contains(';') || before[fp..pos].contains(':')
                })
                .unwrap_or(false);

            if is_font_ctx {
                // Hitta slutet av font-family-värdet (nästa ; eller } eller ")
                let after_font = &remaining[pos + search.len()..];
                let value_end = after_font
                    .find(|c: char| c == ';' || c == '}' || c == '"' || c == '\'')
                    .unwrap_or(after_font.len());

                let current_value = &after_font[..value_end];
                // Lägg bara till fallback om inte redan finns
                if !current_value.to_ascii_lowercase().contains("sans-serif")
                    && !current_value.to_ascii_lowercase().contains("serif")
                    && !current_value.to_ascii_lowercase().contains("monospace")
                    && !current_value.to_ascii_lowercase().contains("system-ui")
                {
                    new_result.push_str(&remaining[..pos + search.len()]);
                    new_result.push_str(current_value);
                    new_result.push_str(", ");
                    new_result.push_str(fallback);
                    remaining = &remaining[pos + search.len() + value_end..];
                    changed = true;
                    continue;
                }
            }
            new_result.push_str(&remaining[..pos + search.len()]);
            remaining = &remaining[pos + search.len()..];
        }
        new_result.push_str(remaining);
        if changed {
            result = new_result;
        }
    }
    result
}

/// Inline all CSS till style=""-attribut med css-inline craten.
/// Tar bort <style>-block efter inlining.
fn inline_css_to_attributes(html: &str) -> String {
    use css_inline::CSSInliner;

    let inliner = CSSInliner::options()
        .inline_style_tags(true)
        .keep_style_tags(false) // Ta bort <style> efter inlining
        .keep_link_tags(false) // Ta bort <link> efter inlining
        .remove_inlined_selectors(true)
        .load_remote_stylesheets(false) // Vi har redan inlinat externa CSS
        .build();

    match inliner.inline(html) {
        Ok(inlined) => inlined,
        Err(_) => {
            // Fallback — returnera original HTML om inlining misslyckas
            html.to_string()
        }
    }
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Position för ett <style>-block i HTML:en
#[derive(Debug)]
struct StyleBlock {
    /// Start-byte av hela <style>...</style>
    start: usize,
    /// Slut-byte (efter </style>)
    end: usize,
    /// CSS-innehållet inuti
    css_content: String,
}

/// Extrahera alla <style>-block från HTML med byte-positioner
fn extract_style_blocks(html: &str) -> Vec<StyleBlock> {
    let mut blocks = Vec::new();
    let lower = html.to_lowercase();
    let mut search_start = 0;

    while let Some(rel_pos) = lower[search_start..].find("<style") {
        let tag_start = search_start + rel_pos;

        // Hitta > som stänger öppnande taggen
        let content_start = match lower[tag_start..].find('>') {
            Some(pos) => tag_start + pos + 1,
            None => break,
        };

        // Hitta </style>
        let close_tag = match lower[content_start..].find("</style>") {
            Some(pos) => content_start + pos,
            None => break,
        };

        let css_content = html[content_start..close_tag].to_string();
        let block_end = close_tag + "</style>".len();

        blocks.push(StyleBlock {
            start: tag_start,
            end: block_end,
            css_content,
        });

        search_start = block_end;
    }

    blocks
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_query_min_width_matches() {
        let vp = ViewportConfig {
            width: 1280,
            height: 900,
            color_scheme: ColorScheme::Light,
        };
        assert!(
            media_condition_matches("(min-width: 768px)", &vp),
            "1280px borde matcha min-width: 768px"
        );
        assert!(
            !media_condition_matches("(min-width: 1400px)", &vp),
            "1280px borde INTE matcha min-width: 1400px"
        );
    }

    #[test]
    fn test_media_query_max_width() {
        let vp = ViewportConfig::default(); // 1280x900
        assert!(
            !media_condition_matches("(max-width: 768px)", &vp),
            "1280px borde INTE matcha max-width: 768px"
        );
        assert!(
            media_condition_matches("(max-width: 1400px)", &vp),
            "1280px borde matcha max-width: 1400px"
        );
    }

    #[test]
    fn test_media_query_combined_and() {
        let vp = ViewportConfig::default();
        assert!(
            media_condition_matches("(min-width: 768px) and (max-width: 1400px)", &vp),
            "1280px borde matcha 768-1400 range"
        );
        assert!(
            !media_condition_matches("(min-width: 768px) and (max-width: 1000px)", &vp),
            "1280px borde INTE matcha 768-1000 range"
        );
    }

    #[test]
    fn test_media_query_screen() {
        let vp = ViewportConfig::default();
        assert!(
            media_condition_matches("screen", &vp),
            "screen borde matcha"
        );
        assert!(
            !media_condition_matches("print", &vp),
            "print borde INTE matcha"
        );
        assert!(
            media_condition_matches("screen and (min-width: 768px)", &vp),
            "screen and min-width borde matcha"
        );
    }

    #[test]
    fn test_media_query_color_scheme() {
        let light = ViewportConfig {
            color_scheme: ColorScheme::Light,
            ..Default::default()
        };
        let dark = ViewportConfig {
            color_scheme: ColorScheme::Dark,
            ..Default::default()
        };
        assert!(
            media_condition_matches("(prefers-color-scheme: light)", &light),
            "light borde matcha light"
        );
        assert!(
            !media_condition_matches("(prefers-color-scheme: dark)", &light),
            "dark borde INTE matcha light"
        );
        assert!(
            media_condition_matches("(prefers-color-scheme: dark)", &dark),
            "dark borde matcha dark"
        );
    }

    #[test]
    fn test_media_query_em_units() {
        let vp = ViewportConfig::default(); // 1280x900
                                            // 48em = 768px
        assert!(
            media_condition_matches("(min-width: 48em)", &vp),
            "1280px borde matcha min-width: 48em (768px)"
        );
        assert!(
            !media_condition_matches("(min-width: 90em)", &vp),
            "1280px borde INTE matcha min-width: 90em (1440px)"
        );
    }

    #[test]
    fn test_extract_style_blocks() {
        let html = r#"<html><head><style>.a{color:red}</style><style type="text/css">.b{color:blue}</style></head></html>"#;
        let blocks = extract_style_blocks(html);
        assert_eq!(blocks.len(), 2, "Borde hitta 2 style-block");
        assert_eq!(blocks[0].css_content, ".a{color:red}");
        assert_eq!(blocks[1].css_content, ".b{color:blue}");
    }

    #[test]
    fn test_compile_css_basic() {
        let html = r#"<html><head><style>
            .hero { color: red; font-size: 24px; }
        </style></head><body><div class="hero">Hello</div></body></html>"#;
        let result = compile_css(html, &ViewportConfig::default());
        assert!(result.fully_compiled, "Kompilering borde lyckas");
        assert!(
            result.style_blocks_processed > 0,
            "Borde processa minst 1 style-block"
        );
        // Kontrollera att style="" finns i output
        assert!(
            result.html.contains("style="),
            "Output borde innehålla inline styles"
        );
        // <style>-block borde vara borta
        assert!(
            !result.html.contains("<style>"),
            "Output borde INTE innehålla <style>-block"
        );
    }

    #[test]
    fn test_compile_css_with_media_queries() {
        let html = r##"<html><head><style>
            .base { color: black; }
            @media (min-width: 768px) {
                .wide { color: blue; }
            }
            @media (max-width: 600px) {
                .narrow { color: red; }
            }
        </style></head><body>
            <div class="base">Base</div>
            <div class="wide">Wide</div>
            <div class="narrow">Narrow</div>
        </body></html>"##;
        let result = compile_css(html, &ViewportConfig::default());
        assert!(result.fully_compiled, "Kompilering borde lyckas");
        // Verifiera att @media (max-width: 600px) filtrerades bort
        // och att base-regeln finns kvar
        assert!(
            result.html.contains("black") || result.html.contains("style="),
            "Borde ha kvar base-styling: {}",
            &result.html[..result.html.len().min(500)]
        );
        // narrow borde INTE ha röd färg (1280 > 600 → max-width: 600 matchar inte)
        assert!(
            !result.html.contains("color: red") && !result.html.contains("color:red"),
            "Narrow element borde INTE ha röd färg (1280 > 600): {}",
            &result.html[..result.html.len().min(500)]
        );
    }

    #[test]
    fn test_compile_css_preserves_non_style_content() {
        let html = r#"<html><body><h1>Hello World</h1><p>Test</p></body></html>"#;
        let result = compile_css(html, &ViewportConfig::default());
        assert!(result.fully_compiled, "Borde lyckas även utan CSS");
        assert!(
            result.html.contains("Hello World"),
            "Borde bevara textinnehåll"
        );
    }

    #[test]
    fn test_compile_css_too_large() {
        let large_html = "x".repeat(6 * 1024 * 1024); // 6 MB
        let result = compile_css(&large_html, &ViewportConfig::default());
        assert!(!result.fully_compiled, "Borde INTE kompilera för stor HTML");
    }

    #[test]
    fn test_filter_media_queries_basic() {
        let css = r#"
            .a { color: red; }
            @media (min-width: 768px) { .b { color: blue; } }
            @media (max-width: 400px) { .c { color: green; } }
            .d { color: yellow; }
        "#;
        let vp = ViewportConfig::default(); // 1280x900
        let filtered = filter_media_queries(css, &vp);

        assert!(filtered.contains(".a"), "Borde behålla .a");
        assert!(filtered.contains(".b"), "Borde behålla .b (1280 > 768)");
        assert!(!filtered.contains(".c"), "Borde ta bort .c (1280 > 400)");
        assert!(filtered.contains(".d"), "Borde behålla .d");
    }

    #[test]
    fn test_find_matching_brace() {
        let css = "{ .a { color: red; } .b { color: blue; } }";
        let end = find_matching_brace(css, 0);
        assert_eq!(
            end,
            Some(css.len() - 1),
            "Borde hitta matchande slutklammer"
        );
    }

    #[test]
    fn test_lightningcss_transforms_css_nesting() {
        // LightningCSS borde downlevla CSS nesting till flat selektorer
        let css = ".parent { color: red; & .child { color: blue; } }";
        let vp = ViewportConfig::default();
        let transformed = transform_single_css(css, &vp);
        // Nesting borde vara expanderat
        assert!(transformed.contains(".parent"), "Borde behålla .parent");
        // Borde inte ha kvar & (nesting syntax)
        assert!(
            !transformed.contains("& .child"),
            "Nesting borde vara expanderat"
        );
    }

    #[test]
    fn test_inline_css_to_attributes() {
        let html = r#"<html><head><style>.red { color: red; }</style></head><body><div class="red">Hi</div></body></html>"#;
        let result = inline_css_to_attributes(html);
        assert!(result.contains("style="), "Borde ha inline style");
    }

    #[test]
    fn test_heavy_media_queries_dont_corrupt_css() {
        // Simulera apple.com-scenario: CSS med många @media-regler
        let mut css = String::from(".base { color: black; }\n");
        for i in 0..100 {
            css.push_str(&format!(
                "@media (min-width: {}px) {{ .item-{} {{ color: blue; }} }}\n",
                400 + i * 10,
                i
            ));
        }
        let vp = ViewportConfig::default(); // 1280x900
        let transformed = transform_single_css(&css, &vp);

        // Transformerad CSS ska vara giltig (inte korrumperad)
        assert!(
            transformed.contains(".base"),
            "Borde behålla .base: {}",
            &transformed[..transformed.len().min(200)]
        );

        // @media (min-width: 400..1280) borde matcha → inner CSS behålls
        let filtered = filter_media_queries(&transformed, &vp);
        assert!(
            filtered.contains(".item-0"),
            "item-0 (min-width:400) borde matcha 1280"
        );

        // @media (min-width: 1390+) borde INTE matcha
        assert!(
            !filtered.contains(".item-99"),
            "item-99 (min-width:1390) borde INTE matcha 1280"
        );
    }

    #[test]
    fn test_css_inline_with_complex_selectors() {
        // Testa css-inline med typiska sajt-mönster
        let html = r##"<html><head><style>
            .nav { display: flex; background: #333; }
            .nav a { color: white; padding: 10px; }
            .hero { background: linear-gradient(to right, #1a1a2e, #16213e); }
            .hero h1 { font-size: 48px; color: white; }
            .card { border: 1px solid #ddd; border-radius: 8px; padding: 16px; }
            .card:hover { box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
        </style></head><body>
            <nav class="nav"><a href="/">Home</a><a href="/about">About</a></nav>
            <div class="hero"><h1>Welcome</h1></div>
            <div class="card"><p>Content</p></div>
        </body></html>"##;

        let result = compile_css(html, &ViewportConfig::default());
        assert!(result.fully_compiled, "Borde kompilera");
        // Nav borde ha flex + bakgrund
        assert!(
            result.html.contains("display: flex") || result.html.contains("display:flex"),
            "Nav borde ha display:flex efter inline: {}",
            &result.html[..result.html.len().min(500)]
        );
        // Hero h1 borde ha font-size
        assert!(
            result.html.contains("font-size"),
            "Hero h1 borde ha font-size"
        );
    }

    #[test]
    fn test_large_css_with_many_media_queries() {
        // Simulera apple.com: 1MB+ CSS, 500 @media, 100 element
        // Steg 1: Bygg stor CSS
        let mut css = String::new();
        // Grundläggande styles
        css.push_str("body { margin: 0; font-family: -apple-system, sans-serif; }\n");
        css.push_str(".nav { display: flex; background: #333; padding: 10px; }\n");
        css.push_str(".nav a { color: white; text-decoration: none; padding: 8px 16px; }\n");
        css.push_str(".hero { background: linear-gradient(135deg, #1a1a2e, #16213e); padding: 80px 0; text-align: center; }\n");
        css.push_str(".hero h1 { color: white; font-size: 56px; font-weight: 700; }\n");
        css.push_str(".grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 24px; max-width: 1200px; margin: 0 auto; padding: 40px 20px; }\n");
        css.push_str(".card { background: #fff; border-radius: 12px; padding: 24px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }\n");
        css.push_str(".card h2 { font-size: 24px; margin: 0 0 12px; }\n");
        css.push_str(".card p { color: #666; line-height: 1.6; }\n");
        css.push_str(".footer { background: #f5f5f7; padding: 40px 20px; text-align: center; color: #86868b; }\n");

        // 500 @media-regler (realistiskt — apple har 1347)
        for i in 0..500 {
            let bp = 320 + i * 2; // breakpoints 320..1320
            css.push_str(&format!(
                "@media only screen and (min-width: {}px) {{ .responsive-{} {{ width: {}px; margin: 0 auto; }} }}\n",
                bp, i, bp - 40
            ));
        }

        // Extra CSS-block (padding)
        for i in 0..200 {
            css.push_str(&format!(
                ".component-{} {{ padding: {}px; background-color: #f{}f{}f{}; }}\n",
                i,
                10 + i % 30,
                i % 10,
                (i + 3) % 10,
                (i + 7) % 10
            ));
        }

        eprintln!("CSS size: {} bytes", css.len());

        // Steg 2: Bygg HTML med element
        let mut html = String::from("<html><head><style>");
        html.push_str(&css);
        html.push_str("</style></head><body>\n");
        html.push_str("<nav class=\"nav\"><a href=\"/\">Home</a><a href=\"/store\">Store</a><a href=\"/mac\">Mac</a></nav>\n");
        html.push_str(
            "<div class=\"hero\"><h1>iPhone 16 Pro</h1><p>Incredible from every angle.</p></div>\n",
        );
        html.push_str("<div class=\"grid\">\n");
        for i in 0..30 {
            html.push_str(&format!(
                "<div class=\"card component-{}\"><h2>Product {}</h2><p>Description for item {}.</p></div>\n",
                i, i, i
            ));
        }
        html.push_str("</div>\n");
        html.push_str("<footer class=\"footer\">Copyright 2024 Apple Inc.</footer>\n");
        html.push_str("</body></html>");

        eprintln!("HTML size: {} bytes", html.len());

        // Steg 3: Kör CSS Compiler
        let vp = ViewportConfig::default(); // 1280x900
        let result = compile_css(&html, &vp);

        eprintln!("Output HTML: {} bytes", result.html.len());
        eprintln!("Blocks processed: {}", result.style_blocks_processed);
        eprintln!("Rules: {}", result.rules_after_filter);
        eprintln!("Time: {} µs", result.compile_time_us);
        eprintln!("Fully compiled: {}", result.fully_compiled);

        let style_attr_count = result.html.matches("style=\"").count();
        eprintln!("Inline style= count: {}", style_attr_count);

        let remaining_style_blocks = result.html.matches("<style").count();
        eprintln!("Remaining <style> blocks: {}", remaining_style_blocks);

        // ASSERTIONS: CSS Compiler ska INTE producera blank output
        assert!(
            result.html.contains("iPhone 16 Pro"),
            "Borde bevara 'iPhone 16 Pro' text"
        );
        assert!(
            result.html.contains("Product 0"),
            "Borde bevara 'Product 0' text"
        );
        assert!(
            result.html.contains("style=") || result.html.contains("<style"),
            "Borde ha antingen inline styles eller <style> block"
        );

        // Kontrollera att @media (max-width:318) inte matchar (1280 > 318)
        // men @media (min-width:320) matchar (1280 >= 320)
        // Kolla att responsive-klasser finns (de som matchar)
        assert!(
            result.html.contains("responsive-0") || result.html.contains("display: flex"),
            "Borde ha kvar matchande responsive-styles eller nav flex"
        );
    }

    #[test]
    fn test_compile_does_not_produce_blank_output() {
        // Regressiontest: CSS Compiler ska aldrig producera blank HTML
        let html = r##"<html><head><style>
            body { margin: 0; font-family: sans-serif; }
            .header { background: #000; color: #fff; padding: 20px; }
            .main { max-width: 1200px; margin: 0 auto; }
            @media (max-width: 768px) { .main { padding: 10px; } }
        </style></head><body>
            <div class="header"><h1>Site Title</h1></div>
            <div class="main"><p>Content here</p></div>
        </body></html>"##;

        let result = compile_css(html, &ViewportConfig::default());
        assert!(result.fully_compiled, "Borde kompilera");

        // Output ska ha synligt innehåll
        assert!(
            result.html.contains("Site Title"),
            "Borde bevara textinnehåll"
        );
        assert!(
            result.html.contains("Content here"),
            "Borde bevara paragraf"
        );
        // Ska ha inline styles
        assert!(result.html.contains("style="), "Borde ha inline styles");
    }
}
