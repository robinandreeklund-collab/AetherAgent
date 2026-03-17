// Fas 11: Inbyggd YOLOv8-inferens via rten
// Screenshot -> objektdetektering -> bounding boxes -> semantiskt trad

use serde::{Deserialize, Serialize};

use crate::grounding::compute_iou;
use crate::types::{BoundingBox, NodeState, SemanticNode, SemanticTree, TrustLevel};

// --- Konstanter ---

/// YOLOv8 UI-elementklasser
pub const UI_CLASSES: &[&str] = &[
    "button", "input", "link", "icon", "text", "image", "checkbox", "radio", "select", "heading",
];

// --- Typer ---

/// UI element detected in a screenshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDetection {
    /// Detekterad klass (t.ex. "button", "input", "link", "icon", "text", "image")
    pub class: String,
    /// Konfidenspoang (0.0-1.0)
    pub confidence: f32,
    /// Bounding box i pixelkoordinater
    pub bbox: BoundingBox,
}

/// Result from the vision pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResult {
    /// Detekterade UI-element
    pub detections: Vec<UiDetection>,
    /// Semantiskt trad byggt fran detektioner
    pub tree: SemanticTree,
    /// Inferenstid i millisekunder
    pub inference_time_ms: u64,
    /// Preprocessing-tid i millisekunder
    pub preprocess_time_ms: u64,
    /// Antal detektioner fore NMS-filtrering
    pub raw_detection_count: u32,
}

/// Configuration for the vision pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// Konfidenströskel (default 0.25)
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    /// IoU-tröskel för NMS (default 0.45)
    #[serde(default = "default_nms_threshold")]
    pub nms_threshold: f32,
    /// Input-storlek för modellen (default 640)
    #[serde(default = "default_input_size")]
    pub input_size: u32,
    /// Max antal detektioner att returnera (default 100)
    #[serde(default = "default_max_detections")]
    pub max_detections: usize,
}

fn default_confidence_threshold() -> f32 {
    0.25
}

fn default_nms_threshold() -> f32 {
    0.45
}

fn default_input_size() -> u32 {
    640
}

fn default_max_detections() -> usize {
    100
}

impl Default for VisionConfig {
    fn default() -> Self {
        VisionConfig {
            confidence_threshold: default_confidence_threshold(),
            nms_threshold: default_nms_threshold(),
            input_size: default_input_size(),
            max_detections: default_max_detections(),
        }
    }
}

// --- Funktioner som alltid ar tillgangliga ---

/// Non-max suppression on detections
///
/// Removes overlapping detections, keeping only the most confident one
/// when IoU exceeds the threshold.
pub fn nms(detections: &mut Vec<UiDetection>, iou_threshold: f32) {
    // Sortera efter konfidens (hogst forst) med total_cmp for NaN-sakerhet
    detections.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));

    let mut keep = vec![true; detections.len()];

    for i in 0..detections.len() {
        if !keep[i] {
            continue;
        }
        for j in (i + 1)..detections.len() {
            if !keep[j] {
                continue;
            }
            let iou = compute_iou(&detections[i].bbox, &detections[j].bbox);
            if iou > iou_threshold {
                // Ta bort den mindre konfidenta detektionen
                keep[j] = false;
            }
        }
    }

    // Behall bara de som inte filtrerats bort
    let mut idx = 0;
    detections.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

/// Build a semantic tree from vision detections (no feature gate needed)
///
/// Creates a `SemanticTree` where each detection becomes a `SemanticNode`.
/// All nodes are marked as `TrustLevel::Untrusted` since they come from
/// visual inference rather than DOM parsing.
pub fn detections_to_tree(detections: &[UiDetection], goal: &str, url: &str) -> SemanticTree {
    let goal_lower = goal.to_lowercase();
    let nodes: Vec<SemanticNode> = detections
        .iter()
        .enumerate()
        .map(|(i, det)| {
            let role = map_class_to_role(&det.class);
            let action = SemanticNode::infer_action(&role);
            let relevance = compute_detection_relevance(&det.class, &goal_lower);

            SemanticNode {
                id: (i + 1) as u32,
                role,
                label: format!("vision_{} #{}", det.class, i + 1),
                value: None,
                state: NodeState::default_state(),
                action,
                relevance,
                trust: TrustLevel::Untrusted,
                children: vec![],
                html_id: None,
                name: None,
                bbox: Some(det.bbox.clone()),
            }
        })
        .collect();

    SemanticTree {
        url: url.to_string(),
        title: String::new(),
        goal: goal.to_string(),
        nodes,
        injection_warnings: vec![],
        parse_time_ms: 0,
        xhr_intercepted: 0,
        xhr_blocked: 0,
    }
}

/// Mappa detektion-klass till semantisk roll
fn map_class_to_role(class: &str) -> String {
    match class {
        "button" => "button".to_string(),
        "input" => "textbox".to_string(),
        "link" => "link".to_string(),
        "icon" => "img".to_string(),
        "text" => "text".to_string(),
        "image" => "img".to_string(),
        "checkbox" => "checkbox".to_string(),
        "radio" => "radio".to_string(),
        "select" => "select".to_string(),
        "heading" => "heading".to_string(),
        other => other.to_string(),
    }
}

/// Berakna relevans baserat pa klass och mal
fn compute_detection_relevance(class: &str, goal_lower: &str) -> f32 {
    let base = SemanticNode::role_priority(&map_class_to_role(class));

    // Enkel goal-matchning: om klassen namns i malet, hoj relevansen
    let goal_boost = if goal_lower.contains(class) { 0.1 } else { 0.0 };

    (base + goal_boost).min(1.0)
}

// --- Feature-gated funktioner (vision) ---

#[cfg(feature = "vision")]
/// Preprocess a PNG image for YOLOv8 inference
///
/// Decodes the PNG, resizes to `input_size x input_size`, and converts
/// to a normalized RGB f32 tensor in CHW format.
pub fn preprocess_image(png_bytes: &[u8], input_size: u32) -> Result<Vec<f32>, String> {
    use image::ImageReader;
    use std::io::Cursor;

    // Dekoda PNG-bild
    let img = ImageReader::new(Cursor::new(png_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Kunde inte lasa bildformat: {}", e))?
        .decode()
        .map_err(|e| format!("Kunde inte dekoda bild: {}", e))?;

    // Ändra storlek till input_size x input_size
    let resized = img.resize_exact(
        input_size,
        input_size,
        image::imageops::FilterType::Triangle,
    );
    let rgb = resized.to_rgb8();

    // Konvertera till CHW-format (Channel, Height, Width), normaliserat 0-1
    let (w, h) = (input_size as usize, input_size as usize);
    let mut tensor = vec![0.0f32; 3 * h * w];

    for y in 0..h {
        for x in 0..w {
            let pixel = rgb.get_pixel(x as u32, y as u32);
            // CHW-layout: [R-kanal][G-kanal][B-kanal]
            tensor[0 * h * w + y * w + x] = pixel[0] as f32 / 255.0;
            tensor[1 * h * w + y * w + x] = pixel[1] as f32 / 255.0;
            tensor[2 * h * w + y * w + x] = pixel[2] as f32 / 255.0;
        }
    }

    Ok(tensor)
}

#[cfg(feature = "vision")]
/// Run YOLOv8-nano inference on preprocessed image tensor
///
/// Loads the ONNX model, creates the input tensor, runs inference,
/// and post-processes results with confidence thresholding and NMS.
pub fn run_inference(
    model_bytes: &[u8],
    tensor: &[f32],
    input_size: u32,
    config: &VisionConfig,
) -> Result<Vec<UiDetection>, String> {
    use rten::Model;
    use rten_tensor::NdTensor;

    // Ladda modellen fran bytes
    let model = Model::load(model_bytes).map_err(|e| format!("Kunde inte ladda modell: {}", e))?;

    // Skapa input-tensor med form [1, 3, input_size, input_size]
    let size = input_size as usize;
    let input = NdTensor::from_data([1, 3, size, size], tensor.to_vec());

    // Kor inferens
    let input_id = model
        .input_ids()
        .first()
        .copied()
        .ok_or_else(|| "Modellen har inga inputs".to_string())?;
    let output_id = model
        .output_ids()
        .first()
        .copied()
        .ok_or_else(|| "Modellen har inga outputs".to_string())?;

    let result = model
        .run(vec![(input_id, input.into())], vec![output_id], None)
        .map_err(|e| format!("Inferensfel: {}", e))?;

    // Hamta output-tensor
    let output = result
        .first()
        .ok_or_else(|| "Inget output fran modellen".to_string())?;
    let output_tensor = output
        .as_float()
        .ok_or_else(|| "Output ar inte float-tensor".to_string())?;

    // Post-process: YOLOv8 output ar [1, num_classes+4, num_predictions]
    // Forsta 4 rader: cx, cy, w, h
    // Resterande rader: klasskonfidens
    let shape = output_tensor.shape();
    if shape.len() < 3 {
        return Err(format!("Ovantad output-form: {:?}", shape));
    }

    let num_attrs = shape[1]; // 4 + num_classes
    let num_preds = shape[2];
    let num_classes = num_attrs.saturating_sub(4).min(UI_CLASSES.len());

    if num_classes == 0 {
        return Err("Modellen har inga klasser".to_string());
    }

    let data = output_tensor.to_vec();
    let mut detections = Vec::new();

    for pred_idx in 0..num_preds {
        // Hitta basta klassen for denna prediktion
        let mut best_class = 0;
        let mut best_conf = 0.0f32;

        for cls in 0..num_classes {
            let conf = data[(4 + cls) * num_preds + pred_idx];
            if conf > best_conf {
                best_conf = conf;
                best_class = cls;
            }
        }

        if best_conf < config.confidence_threshold {
            continue;
        }

        // Extrahera bounding box (cx, cy, w, h) -> (x, y, w, h)
        let cx = data[0 * num_preds + pred_idx];
        let cy = data[1 * num_preds + pred_idx];
        let w = data[2 * num_preds + pred_idx];
        let h = data[3 * num_preds + pred_idx];

        let x = cx - w / 2.0;
        let y = cy - h / 2.0;

        let class_name = UI_CLASSES.get(best_class).unwrap_or(&"unknown");

        detections.push(UiDetection {
            class: class_name.to_string(),
            confidence: best_conf,
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
        });
    }

    // Applicera NMS
    nms(&mut detections, config.nms_threshold);

    // Begränsa antal detektioner
    detections.truncate(config.max_detections);

    Ok(detections)
}

#[cfg(feature = "vision")]
/// Full pipeline: PNG bytes -> detections -> semantic tree
///
/// Preprocesses the image, runs YOLOv8-nano inference, applies NMS,
/// and builds a semantic tree from the detections.
pub fn detect_ui_elements(
    png_bytes: &[u8],
    model_bytes: &[u8],
    goal: &str,
    config: &VisionConfig,
) -> Result<VisionResult, String> {
    use std::time::Instant;

    // Preprocessing
    let pre_start = Instant::now();
    let tensor = preprocess_image(png_bytes, config.input_size)?;
    let preprocess_time_ms = pre_start.elapsed().as_millis() as u64;

    // Inferens
    let inf_start = Instant::now();
    let raw_detections = run_inference(model_bytes, &tensor, config.input_size, config)?;
    let inference_time_ms = inf_start.elapsed().as_millis() as u64;

    let raw_detection_count = raw_detections.len() as u32;

    // Bygg semantiskt trad
    let tree = detections_to_tree(&raw_detections, goal, "screenshot://local");

    Ok(VisionResult {
        detections: raw_detections,
        tree,
        inference_time_ms,
        preprocess_time_ms,
        raw_detection_count,
    })
}

// Stub nar vision-feature inte ar aktiverat
#[cfg(not(feature = "vision"))]
/// Full pipeline: PNG bytes -> detections -> semantic tree
///
/// This is a stub that returns an error when the `vision` feature is not enabled.
/// Compile with `--features vision` to enable YOLOv8 inference.
pub fn detect_ui_elements(
    _png_bytes: &[u8],
    _model_bytes: &[u8],
    _goal: &str,
    _config: &VisionConfig,
) -> Result<VisionResult, String> {
    Err("Vision not available: compile with --features vision".to_string())
}

// --- Tester ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_config_defaults() {
        let config = VisionConfig::default();
        assert!(
            (config.confidence_threshold - 0.25).abs() < 0.01,
            "Konfidenströskel borde vara 0.25"
        );
        assert!(
            (config.nms_threshold - 0.45).abs() < 0.01,
            "NMS-tröskel borde vara 0.45"
        );
        assert_eq!(config.input_size, 640, "Input-storlek borde vara 640");
        assert_eq!(config.max_detections, 100, "Max detektioner borde vara 100");
    }

    #[test]
    fn test_nms_removes_overlapping() {
        // Tva starkt överlappande detektioner, NMS borde behalla bara den mest konfidenta
        let mut detections = vec![
            UiDetection {
                class: "button".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 40.0,
                },
            },
            UiDetection {
                class: "button".to_string(),
                confidence: 0.7,
                bbox: BoundingBox {
                    x: 15.0,
                    y: 12.0,
                    width: 95.0,
                    height: 38.0,
                },
            },
        ];
        nms(&mut detections, 0.45);
        assert_eq!(
            detections.len(),
            1,
            "NMS borde ta bort överlappande detektion"
        );
        assert!(
            (detections[0].confidence - 0.9).abs() < 0.01,
            "Borde behålla den mest konfidenta"
        );
    }

    #[test]
    fn test_nms_keeps_non_overlapping() {
        // Tva icke-overlappande detektioner, bada borde behalles
        let mut detections = vec![
            UiDetection {
                class: "button".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 10.0,
                    width: 50.0,
                    height: 30.0,
                },
            },
            UiDetection {
                class: "input".to_string(),
                confidence: 0.8,
                bbox: BoundingBox {
                    x: 200.0,
                    y: 200.0,
                    width: 100.0,
                    height: 30.0,
                },
            },
        ];
        nms(&mut detections, 0.45);
        assert_eq!(
            detections.len(),
            2,
            "NMS borde behålla icke-överlappande detektioner"
        );
    }

    #[test]
    fn test_detections_to_tree() {
        let detections = vec![
            UiDetection {
                class: "button".to_string(),
                confidence: 0.95,
                bbox: BoundingBox {
                    x: 100.0,
                    y: 200.0,
                    width: 80.0,
                    height: 30.0,
                },
            },
            UiDetection {
                class: "input".to_string(),
                confidence: 0.88,
                bbox: BoundingBox {
                    x: 50.0,
                    y: 100.0,
                    width: 200.0,
                    height: 25.0,
                },
            },
            UiDetection {
                class: "text".to_string(),
                confidence: 0.75,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 10.0,
                    width: 300.0,
                    height: 20.0,
                },
            },
        ];
        let tree = detections_to_tree(&detections, "buy product", "screenshot://local");
        assert_eq!(tree.nodes.len(), 3, "Borde skapa en nod per detektion");
        assert_eq!(
            tree.nodes[0].role, "button",
            "Forsta noden borde vara button"
        );
        assert!(tree.nodes[0].action.is_some(), "Button borde ha action");
        assert!(tree.nodes[0].bbox.is_some(), "Nod borde ha bbox");
        assert_eq!(
            tree.nodes[0].trust,
            TrustLevel::Untrusted,
            "Vision-noder borde vara Untrusted"
        );
    }

    #[test]
    fn test_detections_to_tree_empty() {
        let tree = detections_to_tree(&[], "goal", "url");
        assert!(tree.nodes.is_empty(), "Tomt input borde ge tomt trad");
    }

    #[test]
    fn test_ui_classes_valid() {
        assert!(UI_CLASSES.contains(&"button"), "Borde innehalla button");
        assert!(UI_CLASSES.contains(&"input"), "Borde innehalla input");
        assert!(UI_CLASSES.contains(&"link"), "Borde innehalla link");
        assert!(UI_CLASSES.len() >= 8, "Borde ha minst 8 UI-klasser");
    }

    #[test]
    fn test_nms_empty_input() {
        // Tom lista borde inte krascha
        let mut detections: Vec<UiDetection> = vec![];
        nms(&mut detections, 0.45);
        assert!(detections.is_empty(), "Tom input borde ge tom output");
    }

    #[test]
    fn test_nms_single_detection() {
        // En enda detektion borde behalles
        let mut detections = vec![UiDetection {
            class: "button".to_string(),
            confidence: 0.9,
            bbox: BoundingBox {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 30.0,
            },
        }];
        nms(&mut detections, 0.45);
        assert_eq!(detections.len(), 1, "Enstaka detektion borde behalles");
    }

    #[test]
    fn test_map_class_to_role() {
        assert_eq!(map_class_to_role("button"), "button");
        assert_eq!(map_class_to_role("input"), "textbox");
        assert_eq!(map_class_to_role("link"), "link");
        assert_eq!(map_class_to_role("icon"), "img");
        assert_eq!(map_class_to_role("checkbox"), "checkbox");
        assert_eq!(map_class_to_role("select"), "select");
        assert_eq!(
            map_class_to_role("unknown_class"),
            "unknown_class",
            "Okand klass borde atervandas som den ar"
        );
    }

    #[test]
    fn test_detection_relevance_with_goal_match() {
        // Nar klassen namns i malet borde relevansen vara hogre
        let rel_match = compute_detection_relevance("button", "click button");
        let rel_no_match = compute_detection_relevance("button", "read text");
        assert!(
            rel_match > rel_no_match,
            "Mal-matchning borde ge hogre relevans"
        );
    }

    #[test]
    fn test_detections_to_tree_node_ids_sequential() {
        let detections = vec![
            UiDetection {
                class: "button".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 30.0,
                },
            },
            UiDetection {
                class: "link".to_string(),
                confidence: 0.8,
                bbox: BoundingBox {
                    x: 100.0,
                    y: 0.0,
                    width: 60.0,
                    height: 20.0,
                },
            },
        ];
        let tree = detections_to_tree(&detections, "test", "url");
        assert_eq!(tree.nodes[0].id, 1, "Forsta noden borde ha id 1");
        assert_eq!(tree.nodes[1].id, 2, "Andra noden borde ha id 2");
    }

    #[test]
    fn test_vision_config_serde_roundtrip() {
        let config = VisionConfig::default();
        let json = serde_json::to_string(&config).expect("Borde kunna serialisera VisionConfig");
        let deserialized: VisionConfig =
            serde_json::from_str(&json).expect("Borde kunna deserialisera VisionConfig");
        assert!(
            (deserialized.confidence_threshold - config.confidence_threshold).abs() < 0.001,
            "Konfidenströskel borde överleva serde-roundtrip"
        );
    }

    #[cfg(feature = "vision")]
    #[test]
    fn test_detect_ui_elements_without_model_returns_error() {
        let config = VisionConfig::default();
        let result = detect_ui_elements(&[], &[], "goal", &config);
        assert!(result.is_err(), "Tom modell borde ge fel");
    }

    #[cfg(not(feature = "vision"))]
    #[test]
    fn test_detect_without_feature_returns_error() {
        let config = VisionConfig::default();
        let result = detect_ui_elements(&[], &[], "goal", &config);
        assert!(result.is_err(), "Borde ge fel utan vision-feature");
        assert!(
            result.unwrap_err().contains("not available"),
            "Felmeddelande borde nämna 'not available'"
        );
    }
}
