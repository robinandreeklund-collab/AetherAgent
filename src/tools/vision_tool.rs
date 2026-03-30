// Tool 8: vision — All visual processing
//
// Ersätter: tiered_screenshot, parse_screenshot, vision_parse, fetch_vision,
//           ground_semantic_tree, match_bbox_iou, render_html_to_png, render_with_js

use serde::Deserialize;

use super::{build_tree, now_ms, ToolResult};

/// Request-parametrar för vision-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct VisionRequest {
    /// URL att rendera
    #[serde(default)]
    pub url: Option<String>,
    /// HTML att rendera
    #[serde(default)]
    pub html: Option<String>,
    /// Base64 PNG-screenshot
    #[serde(default)]
    pub screenshot_b64: Option<String>,
    /// Mål för relevans
    #[serde(default)]
    pub goal: Option<String>,
    /// Läge: "detect" (default), "screenshot", "ground", "match"
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Bbox-annotationer (för ground)
    #[serde(default)]
    pub annotations: Option<Vec<crate::grounding::BboxAnnotation>>,
    /// Bounding box att matcha (för match)
    #[serde(default)]
    pub bbox: Option<crate::types::BoundingBox>,
    /// Tree JSON (för match-läge)
    #[serde(default)]
    pub tree_json: Option<String>,
    /// Viewport bredd
    #[serde(default = "default_width")]
    pub width: u32,
    /// Viewport höjd
    #[serde(default = "default_height")]
    pub height: u32,
    /// Streaming
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_mode() -> String {
    "detect".to_string()
}
fn default_width() -> u32 {
    1280
}
fn default_height() -> u32 {
    720
}
fn default_true() -> bool {
    true
}

/// Kör vision-verktyget
pub fn execute(req: &VisionRequest) -> ToolResult {
    let start = now_ms();

    match req.mode.as_str() {
        "screenshot" => execute_screenshot(req, start),
        "detect" => execute_detect(req, start),
        "ground" => execute_ground(req, start),
        "match" => execute_match(req, start),
        other => ToolResult::err(
            format!(
                "Okänt mode: '{other}'. Använd 'detect', 'screenshot', 'ground', eller 'match'."
            ),
            now_ms() - start,
        ),
    }
}

/// Ta screenshot med tiered backend
fn execute_screenshot(req: &VisionRequest, start: u64) -> ToolResult {
    #[cfg(feature = "blitz")]
    {
        let html = match &req.html {
            Some(h) => h.as_str(),
            None => {
                return ToolResult::err(
                    "html krävs för mode=screenshot (synkront). Använd url via HTTP-endpoint.",
                    now_ms() - start,
                )
            }
        };

        let url = req.url.as_deref().unwrap_or("");
        let goal = req.goal.as_deref().unwrap_or("");

        let backend = crate::vision_backend::TieredBackend::new(false);
        let request = crate::vision_backend::ScreenshotRequest {
            html: Some(html.to_string()),
            url: url.to_string(),
            width: req.width,
            height: req.height,
            fast_render: true,
            tier_hint: crate::vision_backend::TierHint::TryBlitzFirst,
            goal: goal.to_string(),
        };

        match backend.screenshot(&request) {
            Ok(result) => {
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&result.png_bytes);
                let data = serde_json::json!({
                    "mode": "screenshot",
                    "tier": format!("{:?}", result.tier_used),
                    "png_b64_length": b64.len(),
                    "png_b64": b64,
                    "width": req.width,
                    "height": req.height,
                });
                ToolResult::ok(data, now_ms() - start)
            }
            Err(e) => ToolResult::err(format!("Screenshot misslyckades: {e}"), now_ms() - start),
        }
    }

    #[cfg(not(feature = "blitz"))]
    {
        let _ = req;
        ToolResult::err("Screenshot kräver --features blitz", now_ms() - start)
    }
}

/// Screenshot + YOLO-detektion
fn execute_detect(req: &VisionRequest, start: u64) -> ToolResult {
    #[cfg(feature = "vision")]
    {
        // Om screenshot redan finns, kör YOLO direkt
        if let Some(ref b64) = req.screenshot_b64 {
            return run_yolo_on_b64(b64, req.goal.as_deref().unwrap_or(""), start);
        }

        // Annars: behöver rendera HTML → screenshot → YOLO
        #[cfg(feature = "blitz")]
        {
            let html = match &req.html {
                Some(h) => h.as_str(),
                None => {
                    return ToolResult::err(
                        "html eller screenshot_b64 krävs för mode=detect",
                        now_ms() - start,
                    )
                }
            };

            let url = req.url.as_deref().unwrap_or("");
            let goal = req.goal.as_deref().unwrap_or("");

            // Rendera via Blitz
            match crate::render_html_to_png(html, url, req.width, req.height, true) {
                Ok(png_bytes) => {
                    let model_path = std::env::var("AETHER_MODEL_PATH")
                        .unwrap_or_else(|_| "yolov8n-ui.onnx".to_string());
                    let model_bytes = match std::fs::read(&model_path) {
                        Ok(b) => b,
                        Err(e) => {
                            return ToolResult::err(
                                format!("Kunde inte ladda modell: {e}"),
                                now_ms() - start,
                            )
                        }
                    };

                    let config = crate::vision::VisionConfig::default();
                    match crate::vision::detect_ui_elements(&png_bytes, &model_bytes, goal, &config)
                    {
                        Ok(result) => {
                            let data = serde_json::to_value(&result)
                                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
                            ToolResult::ok(data, now_ms() - start)
                        }
                        Err(e) => ToolResult::err(
                            format!("Vision-detektion misslyckades: {e}"),
                            now_ms() - start,
                        ),
                    }
                }
                Err(e) => ToolResult::err(format!("Rendering misslyckades: {e}"), now_ms() - start),
            }
        }

        #[cfg(not(feature = "blitz"))]
        {
            ToolResult::err(
                "Rendering kräver --features blitz. Skicka screenshot_b64 istället.",
                now_ms() - start,
            )
        }
    }

    #[cfg(not(feature = "vision"))]
    {
        let _ = req;
        ToolResult::err(
            "Vision-detektion kräver --features vision",
            now_ms() - start,
        )
    }
}

/// Grounding: kombinera semantiskt träd med bbox-annotationer
fn execute_ground(req: &VisionRequest, start: u64) -> ToolResult {
    let html = match &req.html {
        Some(h) => h.as_str(),
        None => return ToolResult::err("html krävs för mode=ground", now_ms() - start),
    };
    let goal = req.goal.as_deref().unwrap_or("");
    let url = req.url.as_deref().unwrap_or("");

    let annotations = match &req.annotations {
        Some(a) => a.clone(),
        None => return ToolResult::err("annotations krävs för mode=ground", now_ms() - start),
    };

    let tree = build_tree(html, goal, url);
    let result = crate::grounding::ground_tree(&tree, &annotations);
    let data = serde_json::to_value(&result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms() - start)
}

/// Match bbox mot noder i träd via IoU
fn execute_match(req: &VisionRequest, start: u64) -> ToolResult {
    let tree_json = match &req.tree_json {
        Some(t) => t.as_str(),
        None => return ToolResult::err("tree_json krävs för mode=match", now_ms() - start),
    };

    let bbox = match &req.bbox {
        Some(b) => b.clone(),
        None => return ToolResult::err("bbox krävs för mode=match", now_ms() - start),
    };

    let tree: crate::types::SemanticTree = match serde_json::from_str(tree_json) {
        Ok(t) => t,
        Err(e) => return ToolResult::err(format!("Ogiltig tree_json: {e}"), now_ms() - start),
    };

    let all_nodes = collect_all_nodes_flat(&tree.nodes);
    let matches = crate::grounding::match_by_iou(&all_nodes, &bbox);
    let data = serde_json::to_value(&matches)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms() - start)
}

/// Kör YOLO på base64-screenshot
#[cfg(feature = "vision")]
fn run_yolo_on_b64(b64: &str, goal: &str, start: u64) -> ToolResult {
    use base64::Engine;
    let png_bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
        Ok(b) => b,
        Err(e) => return ToolResult::err(format!("Invalid base64: {e}"), now_ms() - start),
    };

    let model_path =
        std::env::var("AETHER_MODEL_PATH").unwrap_or_else(|_| "yolov8n-ui.onnx".to_string());
    let model_bytes = match std::fs::read(&model_path) {
        Ok(b) => b,
        Err(e) => {
            return ToolResult::err(format!("Kunde inte ladda modell: {e}"), now_ms() - start)
        }
    };

    let config = crate::vision::VisionConfig::default();
    match crate::vision::detect_ui_elements(&png_bytes, &model_bytes, goal, &config) {
        Ok(result) => {
            let data = serde_json::to_value(&result)
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            ToolResult::ok(data, now_ms() - start)
        }
        Err(e) => ToolResult::err(format!("Vision misslyckades: {e}"), now_ms() - start),
    }
}

/// Platta ut alla noder rekursivt
fn collect_all_nodes_flat(nodes: &[crate::types::SemanticNode]) -> Vec<crate::types::SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node.clone_shallow());
        result.extend(collect_all_nodes_flat(&node.children));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_ground() {
        let html = r##"<html><body>
        <button id="buy-btn">Köp</button>
        <input id="search" type="text" placeholder="Sök">
        </body></html>"##;

        let annotations = vec![crate::grounding::BboxAnnotation {
            html_id: Some("buy-btn".to_string()),
            role: Some("button".to_string()),
            label: Some("Köp".to_string()),
            bbox: crate::types::BoundingBox {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            },
        }];

        let req = VisionRequest {
            html: Some(html.to_string()),
            url: None,
            screenshot_b64: None,
            goal: Some("köp produkt".to_string()),
            mode: "ground".to_string(),
            annotations: Some(annotations),
            bbox: None,
            tree_json: None,
            width: 1280,
            height: 720,
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Ground ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_vision_ground_no_annotations() {
        let req = VisionRequest {
            html: Some("<html><body></body></html>".to_string()),
            url: None,
            screenshot_b64: None,
            goal: None,
            mode: "ground".to_string(),
            annotations: None,
            bbox: None,
            tree_json: None,
            width: 1280,
            height: 720,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ground utan annotations ska ge fel");
    }

    #[test]
    fn test_vision_unknown_mode() {
        let req = VisionRequest {
            html: Some("<html></html>".to_string()),
            url: None,
            screenshot_b64: None,
            goal: None,
            mode: "teleport".to_string(),
            annotations: None,
            bbox: None,
            tree_json: None,
            width: 1280,
            height: 720,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänt mode ska ge fel");
    }

    #[test]
    fn test_vision_match_no_tree() {
        let req = VisionRequest {
            html: None,
            url: None,
            screenshot_b64: None,
            goal: None,
            mode: "match".to_string(),
            annotations: None,
            bbox: Some(crate::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 50.0,
                height: 50.0,
            }),
            tree_json: None,
            width: 1280,
            height: 720,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Match utan tree_json ska ge fel");
    }

    #[test]
    fn test_vision_screenshot_no_blitz() {
        let req = VisionRequest {
            html: Some("<html><body><h1>Test</h1></body></html>".to_string()),
            url: None,
            screenshot_b64: None,
            goal: None,
            mode: "screenshot".to_string(),
            annotations: None,
            bbox: None,
            tree_json: None,
            width: 800,
            height: 600,
            stream: false,
        };
        let result = execute(&req);
        // Resultatet beror på feature flags — ska inte ge panik
        assert!(
            result.error.is_some() || result.data.is_some(),
            "Ska antingen lyckas eller ge feature-fel"
        );
    }
}
