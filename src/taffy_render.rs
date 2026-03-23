/// Taffy + vello_cpu rendering pipeline — fallback-renderer
///
/// HTML → markup5ever DOM → inline-styles → Taffy layout → vello_cpu rasterisering → PNG.
/// Stödjer block/flex layout, bakgrundsfärger, text, borders, padding, margin.
/// Text renderas med riktiga glypher via skrifa charmap + vello_cpu glyph_run.
use crate::parser;
use markup5ever_rcdom::{Handle, NodeData};
use skrifa::instance::LocationRef;
use skrifa::MetadataProvider;
use std::collections::HashMap;
use std::sync::Arc;
use taffy::prelude::*;
use vello_cpu::peniko::{Blob, FontData};

// ── Renderträd ──

/// En nod i renderträdet — kopplar Taffy-layout till visuella egenskaper.
struct RenderNode {
    taffy_id: taffy::NodeId,
    bg_color: Option<[u8; 4]>,
    text_color: [u8; 4],
    font_size: f32,
    text: Option<String>,
    border_color: Option<[u8; 4]>,
    border_width: f32,
    children: Vec<RenderNode>,
}

/// Sökvägar för systemfonter — testar i ordning tills en hittas.
const FONT_SEARCH_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "C:\\Windows\\Fonts\\arial.ttf",
];

/// Ladda en systemfont — returnerar (font-bytes, FontData för vello_cpu).
fn load_system_font() -> Option<(Arc<Vec<u8>>, FontData)> {
    for path in FONT_SEARCH_PATHS {
        if let Ok(bytes) = std::fs::read(path) {
            let arc_bytes = Arc::new(bytes);
            let blob = Blob::new(Arc::new(arc_bytes.as_slice().to_vec().into_boxed_slice()));
            let font_data = FontData::new(blob, 0);
            return Some((arc_bytes, font_data));
        }
    }
    None
}

/// Parsa HTML och rendera till PNG-bytes.
pub fn render_to_png(
    html: &str,
    _base_url: &str,
    width: u32,
    height: u32,
    _fast_render: bool,
) -> Result<Vec<u8>, String> {
    let rcdom = parser::parse_html(html);

    // Ladda systemfont för text-rendering
    let font_info = load_system_font();

    let mut taffy: TaffyTree<()> = TaffyTree::new();

    // Bygg renderträd från DOM — använd fontdata för korrekt textbredd
    let root_render = build_render_tree(&rcdom.document, &mut taffy, width, &font_info);

    // Roten: viewport-storlek
    let root_style = Style {
        size: Size {
            width: length(width as f32),
            height: auto(),
        },
        display: Display::Block,
        ..Default::default()
    };
    let root_id = taffy
        .new_with_children(root_style, &[root_render.taffy_id])
        .map_err(|e| format!("Taffy root: {e}"))?;

    // Layout
    let available = Size {
        width: AvailableSpace::Definite(width as f32),
        height: AvailableSpace::Definite(height as f32),
    };
    taffy
        .compute_layout(root_id, available)
        .map_err(|e| format!("Taffy layout: {e}"))?;

    // Rita med vello_cpu
    let w = width.min(4096) as u16;
    let h = height.min(4096) as u16;
    let mut ctx = vello_cpu::RenderContext::new(w, h);

    // Vit bakgrund
    set_color(&mut ctx, [255, 255, 255, 255]);
    fill_rect(&mut ctx, 0.0, 0.0, w as f64, h as f64);

    // Rita renderträdet
    paint_node(&mut ctx, &taffy, &root_render, 0.0, 0.0, &font_info);

    // Rasterisera till buffer
    let mut buffer = vec![0u8; w as usize * h as usize * 4];
    ctx.render_to_buffer(&mut buffer, w, h, vello_cpu::RenderMode::OptimizeQuality);

    // Koda till PNG
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w as u32, h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header: {e}"))?;
        writer
            .write_image_data(&buffer)
            .map_err(|e| format!("PNG data: {e}"))?;
        writer.finish().map_err(|e| format!("PNG finish: {e}"))?;
    }

    Ok(png_bytes)
}

// ── Hjälpare för vello_cpu ──

fn set_color(ctx: &mut vello_cpu::RenderContext, rgba: [u8; 4]) {
    use vello_cpu::color::{AlphaColor, Srgb};
    let c = AlphaColor::<Srgb>::new([
        rgba[0] as f32 / 255.0,
        rgba[1] as f32 / 255.0,
        rgba[2] as f32 / 255.0,
        rgba[3] as f32 / 255.0,
    ]);
    ctx.set_paint(c);
}

fn fill_rect(ctx: &mut vello_cpu::RenderContext, x0: f64, y0: f64, x1: f64, y1: f64) {
    ctx.fill_rect(&vello_cpu::kurbo::Rect::new(x0, y0, x1, y1));
}

// ── DOM → Renderträd ──

/// Beräkna textbredd med riktiga font-metrics om tillgängligt.
fn measure_text_width(
    text: &str,
    font_size: f32,
    font_info: &Option<(Arc<Vec<u8>>, FontData)>,
) -> f32 {
    if let Some((font_bytes, _)) = font_info {
        if let Ok(font_ref) = skrifa::FontRef::new(font_bytes.as_slice()) {
            let charmap = font_ref.charmap();
            let glyph_metrics = font_ref.glyph_metrics(
                skrifa::instance::Size::new(font_size),
                LocationRef::default(),
            );
            let mut width = 0.0f32;
            for ch in text.chars() {
                if let Some(gid) = charmap.map(ch) {
                    width += glyph_metrics.advance_width(gid).unwrap_or(font_size * 0.5);
                } else {
                    width += font_size * 0.5;
                }
            }
            return width;
        }
    }
    // Fallback utan font
    text.len() as f32 * font_size * 0.6
}

/// Ärvd kontext från förälder-element.
struct InheritedStyle {
    text_color: [u8; 4],
    font_size: f32,
}

impl Default for InheritedStyle {
    fn default() -> Self {
        Self {
            text_color: [0, 0, 0, 255],
            font_size: 16.0,
        }
    }
}

fn build_render_tree(
    handle: &Handle,
    taffy: &mut TaffyTree<()>,
    viewport_width: u32,
    font_info: &Option<(Arc<Vec<u8>>, FontData)>,
) -> RenderNode {
    build_render_tree_inner(
        handle,
        taffy,
        viewport_width,
        font_info,
        &InheritedStyle::default(),
    )
}

fn build_render_tree_inner(
    handle: &Handle,
    taffy: &mut TaffyTree<()>,
    viewport_width: u32,
    font_info: &Option<(Arc<Vec<u8>>, FontData)>,
    inherited: &InheritedStyle,
) -> RenderNode {
    match &handle.data {
        NodeData::Document => {
            let children: Vec<RenderNode> = handle
                .children
                .borrow()
                .iter()
                .map(|c| build_render_tree_inner(c, taffy, viewport_width, font_info, inherited))
                .collect();
            let child_ids: Vec<taffy::NodeId> = children.iter().map(|c| c.taffy_id).collect();
            let style = Style {
                display: Display::Block,
                size: Size {
                    width: percent(1.0),
                    height: auto(),
                },
                ..Default::default()
            };
            let taffy_id = taffy
                .new_with_children(style, &child_ids)
                .unwrap_or_else(|_| taffy.new_leaf(Style::default()).unwrap());
            RenderNode {
                taffy_id,
                bg_color: None,
                text_color: [0, 0, 0, 255],
                font_size: 16.0,
                text: None,
                border_color: None,
                border_width: 0.0,
                children,
            }
        }
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                let taffy_id = taffy
                    .new_leaf(Style {
                        size: Size::zero(),
                        ..Default::default()
                    })
                    .unwrap();
                return RenderNode {
                    taffy_id,
                    bg_color: None,
                    text_color: inherited.text_color,
                    font_size: inherited.font_size,
                    text: None,
                    border_color: None,
                    border_width: 0.0,
                    children: vec![],
                };
            }
            // Textbredd med riktiga font-metrics — ärv font_size från parent
            let font_size = inherited.font_size;
            let text_width = measure_text_width(trimmed, font_size, font_info);
            let vp_w = viewport_width as f32;
            let lines = if vp_w > 0.0 {
                (text_width / vp_w).ceil().max(1.0)
            } else {
                1.0
            };
            let text_height = lines * font_size * 1.2;

            let style = Style {
                size: Size {
                    width: auto(),
                    height: length(text_height),
                },
                min_size: Size {
                    width: auto(),
                    height: length(font_size * 1.2),
                },
                ..Default::default()
            };
            let taffy_id = taffy.new_leaf(style).unwrap();
            RenderNode {
                taffy_id,
                bg_color: None,
                text_color: inherited.text_color,
                font_size,
                text: Some(trimmed.to_string()),
                border_color: None,
                border_width: 0.0,
                children: vec![],
            }
        }
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.to_string().to_lowercase();

            // Skippa osynliga element
            if matches!(
                tag.as_str(),
                "script" | "style" | "meta" | "link" | "head" | "noscript" | "template"
            ) {
                let taffy_id = taffy
                    .new_leaf(Style {
                        display: Display::None,
                        ..Default::default()
                    })
                    .unwrap();
                return RenderNode {
                    taffy_id,
                    bg_color: None,
                    text_color: [0, 0, 0, 255],
                    font_size: 16.0,
                    text: None,
                    border_color: None,
                    border_width: 0.0,
                    children: vec![],
                };
            }

            // Parsa inline styles
            let attrs_ref = attrs.borrow();
            let style_attr = attrs_ref
                .iter()
                .find(|a| &*a.name.local == "style")
                .map(|a| a.value.to_string());

            let parsed_css = style_attr
                .as_deref()
                .map(parse_inline_style)
                .unwrap_or_default();

            // Tagspecifik styling
            let (default_display, default_font_size, _default_font_weight) = tag_defaults(&tag);

            let bg_color = parsed_css
                .get("background-color")
                .and_then(|v| parse_color(v))
                .or_else(|| parsed_css.get("background").and_then(|v| parse_color(v)))
                .or_else(|| tag_default_bg(&tag));
            let text_color = parsed_css
                .get("color")
                .and_then(|v| parse_color(v))
                .or_else(|| tag_default_color(&tag))
                .unwrap_or([0, 0, 0, 255]);
            let font_size = parsed_css
                .get("font-size")
                .and_then(|v| parse_length(v))
                .unwrap_or(default_font_size);
            let border_color = parsed_css
                .get("border-color")
                .and_then(|v| parse_color(v))
                .or_else(|| parsed_css.get("border").and_then(|v| parse_border_color(v)));
            let border_width = parsed_css
                .get("border-width")
                .and_then(|v| parse_length(v))
                .or_else(|| parsed_css.get("border").and_then(|v| parse_border_width(v)))
                .unwrap_or_else(|| tag_default_border(&tag));

            // Ärvd kontext till barn — propagera text_color och font_size
            let child_inherited = InheritedStyle {
                text_color,
                font_size,
            };

            // Bygg barn
            let children: Vec<RenderNode> = handle
                .children
                .borrow()
                .iter()
                .map(|c| {
                    build_render_tree_inner(c, taffy, viewport_width, font_info, &child_inherited)
                })
                .collect();
            let child_ids: Vec<taffy::NodeId> = children.iter().map(|c| c.taffy_id).collect();

            // Display — stöd inline-flex/inline-block som flex
            let display = parsed_css
                .get("display")
                .map(|v| match v.trim() {
                    "flex" | "inline-flex" => Display::Flex,
                    "none" => Display::None,
                    "grid" | "inline-grid" => Display::Grid,
                    "inline" | "inline-block" => Display::Flex,
                    _ => Display::Block,
                })
                .unwrap_or(default_display);

            // Hidden-attribut
            let is_hidden = attrs_ref.iter().any(|a| &*a.name.local == "hidden");
            let is_aria_hidden = attrs_ref
                .iter()
                .any(|a| &*a.name.local == "aria-hidden" && &*a.value == "true");
            let display = if is_hidden || is_aria_hidden {
                Display::None
            } else {
                display
            };

            let padding = parse_box_sides_lp(&parsed_css, "padding");
            let margin = parse_box_sides_lpa(&parsed_css, "margin");

            let width_dim = parsed_css
                .get("width")
                .and_then(|v| parse_dimension(v))
                .unwrap_or_else(auto);
            let height_dim = parsed_css
                .get("height")
                .and_then(|v| parse_dimension(v))
                .unwrap_or_else(auto);
            let max_width = parsed_css
                .get("max-width")
                .and_then(|v| parse_dimension(v))
                .unwrap_or_else(auto);
            let min_width = parsed_css
                .get("min-width")
                .and_then(|v| parse_dimension(v))
                .unwrap_or(length(0.0));
            let max_height = parsed_css
                .get("max-height")
                .and_then(|v| parse_dimension(v))
                .unwrap_or_else(auto);

            // Flex-direction: default Row om CSS säger flex, Column om block
            let flex_direction = parsed_css
                .get("flex-direction")
                .map(|v| match v.trim() {
                    "row" => FlexDirection::Row,
                    "row-reverse" => FlexDirection::RowReverse,
                    "column-reverse" => FlexDirection::ColumnReverse,
                    "column" => FlexDirection::Column,
                    _ => FlexDirection::Row,
                })
                .unwrap_or_else(|| {
                    if display == Display::Flex {
                        FlexDirection::Row
                    } else {
                        FlexDirection::Column
                    }
                });

            // Flex-wrap
            let flex_wrap = parsed_css
                .get("flex-wrap")
                .map(|v| match v.trim() {
                    "wrap" => FlexWrap::Wrap,
                    "wrap-reverse" => FlexWrap::WrapReverse,
                    _ => FlexWrap::NoWrap,
                })
                .unwrap_or(FlexWrap::NoWrap);

            // Justify-content (main axis) — stöder text-align: center via fallback
            let justify_content: Option<JustifyContent> = parsed_css
                .get("justify-content")
                .and_then(|v| parse_justify_content(v))
                .or_else(|| {
                    parsed_css.get("text-align").and_then(|v| match v.trim() {
                        "center" => Some(JustifyContent::Center),
                        "right" | "end" => Some(JustifyContent::End),
                        _ => None,
                    })
                });

            // Align-items (cross axis)
            let align_items = parsed_css
                .get("align-items")
                .and_then(|v| parse_align_items(v));

            // Align-self
            let align_self: Option<AlignSelf> =
                parsed_css.get("align-self").and_then(|v| match v.trim() {
                    "center" => Some(AlignSelf::Center),
                    "flex-start" | "start" => Some(AlignSelf::Start),
                    "flex-end" | "end" => Some(AlignSelf::End),
                    "stretch" => Some(AlignSelf::Stretch),
                    "baseline" => Some(AlignSelf::Baseline),
                    _ => None,
                });

            // Gap
            let gap_val = parsed_css
                .get("gap")
                .and_then(|v| parse_length(v))
                .unwrap_or(0.0);
            let row_gap = parsed_css
                .get("row-gap")
                .and_then(|v| parse_length(v))
                .unwrap_or(gap_val);
            let column_gap = parsed_css
                .get("column-gap")
                .and_then(|v| parse_length(v))
                .unwrap_or(gap_val);

            // Position
            let position = parsed_css
                .get("position")
                .map(|v| match v.trim() {
                    "absolute" => Position::Absolute,
                    _ => Position::Relative,
                })
                .unwrap_or(Position::Relative);

            // Inset (top/right/bottom/left)
            let inset = Rect {
                top: parsed_css
                    .get("top")
                    .and_then(|v| parse_lpa(v))
                    .unwrap_or(LengthPercentageAuto::auto()),
                right: parsed_css
                    .get("right")
                    .and_then(|v| parse_lpa(v))
                    .unwrap_or(LengthPercentageAuto::auto()),
                bottom: parsed_css
                    .get("bottom")
                    .and_then(|v| parse_lpa(v))
                    .unwrap_or(LengthPercentageAuto::auto()),
                left: parsed_css
                    .get("left")
                    .and_then(|v| parse_lpa(v))
                    .unwrap_or(LengthPercentageAuto::auto()),
            };

            // Flex-grow/shrink/basis
            let flex_grow = parsed_css
                .get("flex-grow")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(0.0);
            let flex_shrink = parsed_css
                .get("flex-shrink")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            let flex_basis = parsed_css
                .get("flex-basis")
                .and_then(|v| parse_dimension(v))
                .unwrap_or_else(auto);

            // Overflow — Taffy overflow:hidden ändrar layout-beräkning utan visuell klippning
            // → sätts alltid till Visible för att inte dölja content
            let overflow_x = taffy::Overflow::Visible;
            let overflow_y = taffy::Overflow::Visible;

            let bw = LengthPercentage::length(border_width);
            let mut style = Style {
                display,
                position,
                inset,
                size: Size {
                    width: width_dim,
                    height: height_dim,
                },
                min_size: Size {
                    width: min_width,
                    height: length(0.0),
                },
                max_size: Size {
                    width: max_width,
                    height: max_height,
                },
                padding,
                margin,
                flex_direction,
                flex_wrap,
                flex_grow,
                flex_shrink,
                flex_basis,
                gap: Size {
                    width: length(column_gap),
                    height: length(row_gap),
                },
                overflow: taffy::Point {
                    x: overflow_x,
                    y: overflow_y,
                },
                border: Rect {
                    left: bw,
                    right: bw,
                    top: bw,
                    bottom: bw,
                },
                ..Default::default()
            };
            if let Some(jc) = justify_content {
                style.justify_content = Some(jc);
            }
            if let Some(ai) = align_items {
                style.align_items = Some(ai);
            }
            if let Some(als) = align_self {
                style.align_self = Some(als);
            }

            let taffy_id = taffy
                .new_with_children(style, &child_ids)
                .unwrap_or_else(|_| taffy.new_leaf(Style::default()).unwrap());

            RenderNode {
                taffy_id,
                bg_color,
                text_color,
                font_size,
                text: None,
                border_color,
                border_width,
                children,
            }
        }
        _ => {
            // Kommentar, ProcessingInstruction, etc — osynliga
            let taffy_id = taffy
                .new_leaf(Style {
                    display: Display::None,
                    ..Default::default()
                })
                .unwrap();
            RenderNode {
                taffy_id,
                bg_color: None,
                text_color: [0, 0, 0, 255],
                font_size: 16.0,
                text: None,
                border_color: None,
                border_width: 0.0,
                children: vec![],
            }
        }
    }
}

// ── Taffy → vello_cpu paint ──

fn paint_node(
    ctx: &mut vello_cpu::RenderContext,
    taffy: &TaffyTree<()>,
    node: &RenderNode,
    parent_x: f64,
    parent_y: f64,
    font_info: &Option<(Arc<Vec<u8>>, FontData)>,
) {
    let layout = taffy.layout(node.taffy_id).unwrap();
    let x = parent_x + layout.location.x as f64;
    let y = parent_y + layout.location.y as f64;
    let w = layout.size.width as f64;
    let h = layout.size.height as f64;

    if w < 0.5 || h < 0.5 {
        return;
    }

    // Bakgrundsfärg
    if let Some(bg) = node.bg_color {
        set_color(ctx, bg);
        fill_rect(ctx, x, y, x + w, y + h);
    }

    // Border
    if node.border_width > 0.5 {
        let bc = node.border_color.unwrap_or([0, 0, 0, 255]);
        set_color(ctx, bc);
        let bw = node.border_width as f64;
        fill_rect(ctx, x, y, x + w, y + bw);
        fill_rect(ctx, x, y + h - bw, x + w, y + h);
        fill_rect(ctx, x, y, x + bw, y + h);
        fill_rect(ctx, x + w - bw, y, x + w, y + h);
    }

    // Text — riktig glyph-rendering via skrifa + vello_cpu
    if let Some(ref text) = node.text {
        if !text.is_empty() {
            paint_text_glyphs(ctx, text, node, x, y, w, font_info);
        }
    }

    // Rita barn
    for child in &node.children {
        paint_node(ctx, taffy, child, x, y, font_info);
    }
}

/// Rendera text med riktiga glypher via vello_cpu glyph_run.
fn paint_text_glyphs(
    ctx: &mut vello_cpu::RenderContext,
    text: &str,
    node: &RenderNode,
    x: f64,
    y: f64,
    box_width: f64,
    font_info: &Option<(Arc<Vec<u8>>, FontData)>,
) {
    let font_size = node.font_size;
    let line_height = font_size * 1.2;

    // Sätt textfärg
    set_color(ctx, node.text_color);

    if let Some((font_bytes, font_data)) = font_info {
        // Riktig glyph-rendering
        if let Ok(font_ref) = skrifa::FontRef::new(font_bytes.as_slice()) {
            let charmap = font_ref.charmap();
            let glyph_metrics = font_ref.glyph_metrics(
                skrifa::instance::Size::new(font_size),
                LocationRef::default(),
            );

            // Dela upp text i rader baserat på box-bredd
            let mut lines: Vec<Vec<(vello_cpu::Glyph, f32)>> = Vec::new();
            let mut current_line: Vec<(vello_cpu::Glyph, f32)> = Vec::new();
            let mut cx = 0.0f32;

            for ch in text.chars() {
                if ch == '\n' {
                    lines.push(std::mem::take(&mut current_line));
                    cx = 0.0;
                    continue;
                }

                let gid = charmap.map(ch);
                let advance = gid
                    .and_then(|g| glyph_metrics.advance_width(g))
                    .unwrap_or(font_size * 0.5);

                // Radbrytning
                if cx + advance > box_width as f32 && !current_line.is_empty() {
                    lines.push(std::mem::take(&mut current_line));
                    cx = 0.0;
                }

                if let Some(gid) = gid {
                    current_line.push((
                        vello_cpu::Glyph {
                            id: gid.to_u32(),
                            x: cx,
                            y: 0.0,
                        },
                        advance,
                    ));
                }
                cx += advance;
            }
            if !current_line.is_empty() {
                lines.push(current_line);
            }

            // Rendera varje rad som en glyph_run
            for (line_idx, line_glyphs) in lines.iter().enumerate() {
                if line_glyphs.is_empty() {
                    continue;
                }
                let line_y = y + (line_idx as f64 + 1.0) * line_height as f64;
                let transform = vello_cpu::kurbo::Affine::translate((x, line_y));

                let glyphs_iter = line_glyphs.iter().map(|(g, _)| *g);

                ctx.glyph_run(font_data)
                    .font_size(font_size)
                    .glyph_transform(transform)
                    .hint(true)
                    .fill_glyphs(glyphs_iter);
            }
            return;
        }
    }

    // Fallback: rektanglar om ingen font finns
    let char_w = font_size as f64 * 0.55;
    let char_h = font_size as f64 * 0.7;
    let max_x = x + box_width;
    let mut cx = x;
    let mut cy = y + font_size as f64 * 0.25;

    for ch in text.chars() {
        if ch == '\n' || cx + char_w > max_x {
            cx = x;
            cy += line_height as f64;
            if ch == '\n' {
                continue;
            }
        }
        if !ch.is_whitespace() {
            fill_rect(ctx, cx, cy, cx + char_w * 0.8, cy + char_h);
        }
        cx += char_w;
    }
}

// ── CSS-parsning ──

fn parse_inline_style(style: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for decl in style.split(';') {
        let decl = decl.trim();
        if let Some(colon) = decl.find(':') {
            let prop = decl[..colon].trim().to_lowercase();
            let val = decl[colon + 1..].trim().to_string();
            if !prop.is_empty() && !val.is_empty() {
                map.insert(prop, val);
            }
        }
    }
    map
}

fn parse_color(val: &str) -> Option<[u8; 4]> {
    let val = val.trim();
    match val.to_lowercase().as_str() {
        "transparent" => return Some([0, 0, 0, 0]),
        "black" => return Some([0, 0, 0, 255]),
        "white" => return Some([255, 255, 255, 255]),
        "red" => return Some([255, 0, 0, 255]),
        "green" => return Some([0, 128, 0, 255]),
        "blue" => return Some([0, 0, 255, 255]),
        "yellow" => return Some([255, 255, 0, 255]),
        "orange" => return Some([255, 165, 0, 255]),
        "gray" | "grey" => return Some([128, 128, 128, 255]),
        "lightgray" | "lightgrey" => return Some([211, 211, 211, 255]),
        "darkgray" | "darkgrey" => return Some([169, 169, 169, 255]),
        "navy" => return Some([0, 0, 128, 255]),
        "purple" => return Some([128, 0, 128, 255]),
        "teal" => return Some([0, 128, 128, 255]),
        "silver" => return Some([192, 192, 192, 255]),
        "maroon" => return Some([128, 0, 0, 255]),
        "olive" => return Some([128, 128, 0, 255]),
        "aqua" | "cyan" => return Some([0, 255, 255, 255]),
        "fuchsia" | "magenta" => return Some([255, 0, 255, 255]),
        "lime" => return Some([0, 255, 0, 255]),
        _ => {}
    }
    if let Some(hex) = val.strip_prefix('#') {
        return parse_hex_color(hex);
    }
    if let Some(inner) = val
        .strip_prefix("rgb(")
        .or_else(|| val.strip_prefix("rgba("))
    {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split([',', '/']).map(|s| s.trim()).collect();
        if parts.len() >= 3 {
            let r = parse_color_component(parts[0])?;
            let g = parse_color_component(parts[1])?;
            let b = parse_color_component(parts[2])?;
            let a = if parts.len() >= 4 {
                parse_alpha_component(parts[3])
            } else {
                255
            };
            return Some([r, g, b, a]);
        }
    }
    None
}

fn parse_hex_color(hex: &str) -> Option<[u8; 4]> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some([r * 17, g * 17, b * 17, 255])
        }
        4 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()?;
            Some([r * 17, g * 17, b * 17, a * 17])
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([r, g, b, 255])
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}

fn parse_color_component(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        Some((v * 2.55).round().clamp(0.0, 255.0) as u8)
    } else {
        s.parse::<u8>().ok().or_else(|| {
            s.parse::<f32>()
                .ok()
                .map(|v| v.round().clamp(0.0, 255.0) as u8)
        })
    }
}

fn parse_alpha_component(s: &str) -> u8 {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim()
            .parse::<f32>()
            .map(|v| (v * 2.55).round().clamp(0.0, 255.0) as u8)
            .unwrap_or(255)
    } else {
        s.parse::<f32>()
            .map(|v| {
                if v <= 1.0 {
                    (v * 255.0).round().clamp(0.0, 255.0) as u8
                } else {
                    v.round().clamp(0.0, 255.0) as u8
                }
            })
            .unwrap_or(255)
    }
}

fn parse_length(val: &str) -> Option<f32> {
    let val = val.trim().to_lowercase();
    if let Some(px) = val.strip_suffix("px") {
        return px.trim().parse().ok();
    }
    // Kolla rem FÖRE em — "1.5rem" matchar annars "em"-suffixet
    if let Some(rem) = val.strip_suffix("rem") {
        return rem.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    if let Some(em) = val.strip_suffix("em") {
        return em.trim().parse::<f32>().ok().map(|v| v * 16.0);
    }
    val.parse().ok()
}

fn parse_dimension(val: &str) -> Option<Dimension> {
    let val = val.trim().to_lowercase();
    if val == "auto" {
        return Some(auto());
    }
    if let Some(pct) = val.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        return Some(percent(v / 100.0));
    }
    parse_length(&val).map(length)
}

fn parse_box_sides_lp(css: &HashMap<String, String>, prop: &str) -> Rect<LengthPercentage> {
    if let Some(shorthand) = css.get(prop) {
        return parse_shorthand_box(shorthand);
    }
    let zero = LengthPercentage::length(0.0);
    let top = css
        .get(&format!("{prop}-top"))
        .and_then(|v| parse_lp(v))
        .unwrap_or(zero);
    let right = css
        .get(&format!("{prop}-right"))
        .and_then(|v| parse_lp(v))
        .unwrap_or(zero);
    let bottom = css
        .get(&format!("{prop}-bottom"))
        .and_then(|v| parse_lp(v))
        .unwrap_or(zero);
    let left = css
        .get(&format!("{prop}-left"))
        .and_then(|v| parse_lp(v))
        .unwrap_or(zero);
    Rect {
        top,
        right,
        bottom,
        left,
    }
}

fn parse_box_sides_lpa(css: &HashMap<String, String>, prop: &str) -> Rect<LengthPercentageAuto> {
    if let Some(shorthand) = css.get(prop) {
        return parse_shorthand_box_auto(shorthand);
    }
    let zero = LengthPercentageAuto::length(0.0);
    let top = css
        .get(&format!("{prop}-top"))
        .and_then(|v| parse_lpa(v))
        .unwrap_or(zero);
    let right = css
        .get(&format!("{prop}-right"))
        .and_then(|v| parse_lpa(v))
        .unwrap_or(zero);
    let bottom = css
        .get(&format!("{prop}-bottom"))
        .and_then(|v| parse_lpa(v))
        .unwrap_or(zero);
    let left = css
        .get(&format!("{prop}-left"))
        .and_then(|v| parse_lpa(v))
        .unwrap_or(zero);
    Rect {
        top,
        right,
        bottom,
        left,
    }
}

fn parse_lpa(val: &str) -> Option<LengthPercentageAuto> {
    let val = val.trim();
    if val == "auto" {
        return Some(LengthPercentageAuto::auto());
    }
    if let Some(pct) = val.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        return Some(LengthPercentageAuto::percent(v / 100.0));
    }
    parse_length(val).map(LengthPercentageAuto::length)
}

fn parse_shorthand_box_auto(val: &str) -> Rect<LengthPercentageAuto> {
    let zero = LengthPercentageAuto::length(0.0);
    let parts: Vec<LengthPercentageAuto> = val.split_whitespace().filter_map(parse_lpa).collect();
    match parts.len() {
        1 => Rect {
            top: parts[0],
            right: parts[0],
            bottom: parts[0],
            left: parts[0],
        },
        2 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[0],
            left: parts[1],
        },
        3 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[1],
        },
        4 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[3],
        },
        _ => Rect {
            top: zero,
            right: zero,
            bottom: zero,
            left: zero,
        },
    }
}

fn parse_lp(val: &str) -> Option<LengthPercentage> {
    let val = val.trim();
    if let Some(pct) = val.strip_suffix('%') {
        let v: f32 = pct.trim().parse().ok()?;
        return Some(LengthPercentage::percent(v / 100.0));
    }
    parse_length(val).map(LengthPercentage::length)
}

fn parse_shorthand_box(val: &str) -> Rect<LengthPercentage> {
    let zero = LengthPercentage::length(0.0);
    let parts: Vec<LengthPercentage> = val.split_whitespace().filter_map(parse_lp).collect();
    match parts.len() {
        1 => Rect {
            top: parts[0],
            right: parts[0],
            bottom: parts[0],
            left: parts[0],
        },
        2 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[0],
            left: parts[1],
        },
        3 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[1],
        },
        4 => Rect {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[3],
        },
        _ => Rect {
            top: zero,
            right: zero,
            bottom: zero,
            left: zero,
        },
    }
}

fn parse_border_width(val: &str) -> Option<f32> {
    for part in val.split_whitespace() {
        if let Some(w) = parse_length(part) {
            return Some(w);
        }
    }
    None
}

fn tag_defaults(tag: &str) -> (Display, f32, u16) {
    match tag {
        "div" | "section" | "article" | "main" | "header" | "footer" | "nav" | "aside" | "form"
        | "fieldset" | "details" | "summary" | "figure" | "figcaption" | "blockquote" | "pre"
        | "ul" | "ol" | "dl" | "dt" | "dd" | "table" | "thead" | "tbody" | "tfoot" | "hr"
        | "address" | "hgroup" => (Display::Block, 16.0, 400),
        "p" | "li" | "tr" | "td" | "th" => (Display::Block, 16.0, 400),
        "h1" => (Display::Block, 32.0, 700),
        "h2" => (Display::Block, 24.0, 700),
        "h3" => (Display::Block, 18.72, 700),
        "h4" => (Display::Block, 16.0, 700),
        "h5" => (Display::Block, 13.28, 700),
        "h6" => (Display::Block, 10.72, 700),
        // Inline-element: Flex med Row-direction (Taffy saknar Display::Inline)
        "span" | "a" | "strong" | "em" | "b" | "i" | "u" | "s" | "small" | "sub" | "sup"
        | "code" | "kbd" | "abbr" | "mark" | "time" | "label" => (Display::Flex, 16.0, 400),
        "img" => (Display::Block, 16.0, 400),
        "input" | "select" | "textarea" => (Display::Block, 16.0, 400),
        "button" => (Display::Flex, 16.0, 400),
        "html" | "body" => (Display::Block, 16.0, 400),
        _ => (Display::Block, 16.0, 400),
    }
}

/// Standardfärger för specifika HTML-element.
fn tag_default_color(tag: &str) -> Option<[u8; 4]> {
    match tag {
        "a" => Some([0, 0, 238, 255]), // Blå länkfärg
        _ => None,
    }
}

/// Standard-bakgrundsfärg för element.
fn tag_default_bg(tag: &str) -> Option<[u8; 4]> {
    match tag {
        "button" => Some([239, 239, 239, 255]), // Ljusgrå
        "input" | "textarea" | "select" => Some([255, 255, 255, 255]),
        _ => None,
    }
}

/// Standard border-bredd.
fn tag_default_border(tag: &str) -> f32 {
    match tag {
        "button" | "input" | "textarea" | "select" => 1.0,
        "hr" => 1.0,
        _ => 0.0,
    }
}

/// Parsa border-color från shorthand.
fn parse_border_color(val: &str) -> Option<[u8; 4]> {
    // "1px solid #ccc" → extrahera färg
    for part in val.split_whitespace() {
        if let Some(c) = parse_color(part) {
            return Some(c);
        }
    }
    None
}

fn parse_justify_content(val: &str) -> Option<JustifyContent> {
    match val.trim() {
        "center" => Some(JustifyContent::Center),
        "flex-start" | "start" => Some(JustifyContent::Start),
        "flex-end" | "end" => Some(JustifyContent::End),
        "space-between" => Some(JustifyContent::SpaceBetween),
        "space-around" => Some(JustifyContent::SpaceAround),
        "space-evenly" => Some(JustifyContent::SpaceEvenly),
        _ => None,
    }
}

fn parse_align_items(val: &str) -> Option<AlignItems> {
    match val.trim() {
        "center" => Some(AlignItems::Center),
        "flex-start" | "start" => Some(AlignItems::FlexStart),
        "flex-end" | "end" => Some(AlignItems::FlexEnd),
        "stretch" => Some(AlignItems::Stretch),
        "baseline" => Some(AlignItems::Baseline),
        _ => None,
    }
}

// ── Tester ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple_html() {
        let html = r##"<html><body><h1>Hello</h1><p>World</p></body></html>"##;
        let result = render_to_png(html, "http://example.com", 800, 600, true);
        assert!(result.is_ok(), "Rendering borde lyckas");
        let png = result.unwrap();
        assert!(png.len() > 100, "PNG borde ha rimlig storlek");
        assert_eq!(
            &png[..4],
            &[0x89, b'P', b'N', b'G'],
            "Borde vara giltig PNG"
        );
    }

    #[test]
    fn test_render_styled_div() {
        let html = r##"<div style="background-color: red; width: 200px; height: 100px;"></div>"##;
        let result = render_to_png(html, "http://example.com", 800, 600, true);
        assert!(result.is_ok(), "Styled div borde rendera");
        let png = result.unwrap();
        assert!(png.len() > 100, "PNG borde inte vara tom");
    }

    #[test]
    fn test_render_empty_html() {
        let html = "<html><body></body></html>";
        let result = render_to_png(html, "http://example.com", 800, 600, true);
        assert!(result.is_ok(), "Tom HTML borde rendera (vit sida)");
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#ff0000"), Some([255, 0, 0, 255]));
        assert_eq!(parse_color("#00ff00"), Some([0, 255, 0, 255]));
        assert_eq!(parse_color("#f00"), Some([255, 0, 0, 255]));
        assert_eq!(parse_color("rgb(255, 128, 0)"), Some([255, 128, 0, 255]));
        assert_eq!(parse_color("rgba(255, 0, 0, 0.5)"), Some([255, 0, 0, 128]));
    }

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("red"), Some([255, 0, 0, 255]));
        assert_eq!(parse_color("blue"), Some([0, 0, 255, 255]));
        assert_eq!(parse_color("transparent"), Some([0, 0, 0, 0]));
    }

    #[test]
    fn test_parse_inline_style() {
        let css = parse_inline_style("background-color: red; width: 100px; height: 50px");
        assert_eq!(css.get("background-color"), Some(&"red".to_string()));
        assert_eq!(css.get("width"), Some(&"100px".to_string()));
        assert_eq!(css.get("height"), Some(&"50px".to_string()));
    }

    #[test]
    fn test_parse_length_units() {
        assert_eq!(parse_length("16px"), Some(16.0));
        assert_eq!(parse_length("2em"), Some(32.0));
        assert_eq!(parse_length("1.5rem"), Some(24.0));
        assert_eq!(parse_length("42"), Some(42.0));
    }

    #[test]
    fn test_parse_dimension_values() {
        assert!(matches!(parse_dimension("auto"), Some(d) if d == auto()));
        assert!(matches!(parse_dimension("50%"), Some(d) if d == percent(0.5)));
        assert!(matches!(parse_dimension("100px"), Some(d) if d == length(100.0)));
    }

    #[test]
    fn test_parse_shorthand_box_values() {
        let r = parse_shorthand_box("10px");
        assert_eq!(r.top, LengthPercentage::length(10.0));
        assert_eq!(r.right, LengthPercentage::length(10.0));

        let r2 = parse_shorthand_box("10px 20px");
        assert_eq!(r2.top, LengthPercentage::length(10.0));
        assert_eq!(r2.right, LengthPercentage::length(20.0));
        assert_eq!(r2.bottom, LengthPercentage::length(10.0));
        assert_eq!(r2.left, LengthPercentage::length(20.0));
    }

    #[test]
    fn test_render_with_border() {
        let html = r##"<div style="border: 2px solid black; width: 100px; height: 50px;"></div>"##;
        let result = render_to_png(html, "http://example.com", 400, 300, true);
        assert!(result.is_ok(), "Border-rendering borde lyckas");
    }

    #[test]
    fn test_render_hidden_element() {
        let html = r##"<div hidden>Osynlig</div><div>Synlig</div>"##;
        let result = render_to_png(html, "http://example.com", 400, 300, true);
        assert!(result.is_ok(), "Hidden element borde hanteras");
    }

    #[test]
    fn test_render_nested_elements() {
        let html = r##"
        <div style="background-color: #f0f0f0; padding: 20px;">
            <h1 style="color: navy;">Rubrik</h1>
            <p style="color: #333;">Brödtext här</p>
            <div style="background-color: #e0e0e0; padding: 10px; margin: 5px;">
                <span>Nestad text</span>
            </div>
        </div>
        "##;
        let result = render_to_png(html, "http://example.com", 800, 600, true);
        assert!(result.is_ok(), "Nested rendering borde lyckas");
        let png = result.unwrap();
        assert!(png.len() > 500, "Nested sida borde ha mer innehåll");
    }
}
