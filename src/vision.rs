// Fas 11: Inbyggd YOLOv8-inferens via ONNX Runtime (ort)
// Screenshot -> objektdetektering -> bounding boxes -> semantiskt trad
//
// [RTEN-ROLLBACK-ID:vision-imports] Gamla imports:
// use rten_tensor::{AsView, Layout};

use std::collections::HashMap;

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
    /// Modellversion som användes
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_version: String,
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
    /// Per-klass konfidenströsklar (override global threshold)
    #[serde(default)]
    pub class_thresholds: HashMap<String, f32>,
    /// Dynamiska klassetiketter (override UI_CLASSES om icke-tom)
    #[serde(default)]
    pub class_labels: Vec<String>,
    /// Modellversion för spårbarhet
    #[serde(default)]
    pub model_version: String,
    /// Minsta detektionsarea i pixels² (filtrera bort artefakter)
    #[serde(default)]
    pub min_detection_area: f32,
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
            class_thresholds: HashMap::new(),
            class_labels: Vec::new(),
            model_version: String::new(),
            min_detection_area: 0.0,
        }
    }
}

impl VisionConfig {
    /// Hämta konfidenströskel för en specifik klass
    ///
    /// Returnerar per-klass-tröskeln om den finns, annars global threshold.
    pub fn threshold_for_class(&self, class: &str) -> f32 {
        self.class_thresholds
            .get(class)
            .copied()
            .unwrap_or(self.confidence_threshold)
    }

    /// Hämta klassetikett för ett index
    ///
    /// Returnerar dynamisk etikett om class_labels är satt, annars UI_CLASSES.
    pub fn class_name(&self, index: usize) -> &str {
        if !self.class_labels.is_empty() {
            self.class_labels
                .get(index)
                .map(|s| s.as_str())
                .unwrap_or("unknown")
        } else {
            UI_CLASSES.get(index).copied().unwrap_or("unknown")
        }
    }

    /// Antal klasser
    pub fn num_classes(&self) -> usize {
        if !self.class_labels.is_empty() {
            self.class_labels.len()
        } else {
            UI_CLASSES.len()
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
            let channel_stride = h * w;
            tensor[y * w + x] = pixel[0] as f32 / 255.0;
            tensor[channel_stride + y * w + x] = pixel[1] as f32 / 255.0;
            tensor[2 * channel_stride + y * w + x] = pixel[2] as f32 / 255.0;
        }
    }

    Ok(tensor)
}

// [RTEN-ROLLBACK-ID:vision-core] Gamla rten-baserade funktioner:
// pub fn load_model(model_bytes: &[u8]) -> Result<rten::Model, String> { ... }
// pub fn run_inference_with_model(model: &rten::Model, ...) -> Result<Vec<UiDetection>, String> { ... }
// pub fn run_inference(model_bytes: &[u8], ...) -> Result<Vec<UiDetection>, String> { ... }
// pub fn detect_ui_elements_with_model(png_bytes: &[u8], model: &rten::Model, ...) { ... }
// pub fn detect_ui_elements(png_bytes: &[u8], model_bytes: &[u8], ...) { ... }
// Se git-historik för fullständig implementering (commit före denna)

#[cfg(feature = "vision")]
/// Load an ONNX model into an ORT session (expensive — call once, reuse the result).
///
/// Konfigurerar ONNX Runtime med:
/// - Graph optimization level ALL (op fusion, constant folding)
/// - Inter-op parallelism
/// - Optimerad tråd-pool
pub fn load_model(model_bytes: &[u8]) -> Result<ort::session::Session, String> {
    let session = ort::session::Session::builder()
        .map_err(|e| format!("ORT session builder: {e}"))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ORT optimization level: {e}"))?
        .with_intra_threads(4)
        .map_err(|e| format!("ORT intra threads: {e}"))?
        .with_inter_threads(2)
        .map_err(|e| format!("ORT inter threads: {e}"))?
        .commit_from_memory(model_bytes)
        .map_err(|e| format!("ORT model load: {e}"))?;
    Ok(session)
}

#[cfg(feature = "vision")]
/// Run YOLOv8-nano inference with a pre-loaded ORT session (fast path).
///
/// ORT `run` kräver `&mut Session`. Anroparen ansvarar för synkronisering
/// (t.ex. via Mutex i server state).
pub fn run_inference_with_model(
    session: &mut ort::session::Session,
    tensor: &[f32],
    input_size: u32,
    config: &VisionConfig,
) -> Result<Vec<UiDetection>, String> {
    use ort::value::TensorRef;

    let size = input_size as usize;

    // Skapa input TensorRef (zero-copy view av vår data)
    let input_ref = TensorRef::<f32>::from_array_view(([1usize, 3, size, size], tensor))
        .map_err(|e| format!("ORT tensor create: {e}"))?;

    // Kör inferens
    let outputs = session
        .run(ort::inputs![input_ref])
        .map_err(|e| format!("ORT inference: {e}"))?;

    // Hämta output — YOLOv8 output: [1, num_classes+4, num_predictions]
    let (_name, output_value) = outputs
        .iter()
        .next()
        .ok_or_else(|| "Inget output fran modellen".to_string())?;
    let (shape, data) = output_value
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("ORT output extract: {e}"))?;

    // Shape deref:ar till SmallVec<[i64; 4]>
    if shape.len() < 3 {
        return Err(format!("Ovantad output-form: {:?}", &**shape));
    }

    let num_attrs = shape[1] as usize; // 4 + num_classes
    let num_preds = shape[2] as usize;
    let num_classes = num_attrs.saturating_sub(4).min(config.num_classes());

    if num_classes == 0 {
        return Err("Modellen har inga klasser".to_string());
    }

    // Platt data-access: data har layout [1, num_attrs, num_preds] row-major
    let mut detections = Vec::new();

    for pred_idx in 0..num_preds {
        let mut best_class = 0;
        let mut best_conf = 0.0f32;

        for cls in 0..num_classes {
            let conf = data[(4 + cls) * num_preds + pred_idx];
            if conf > best_conf {
                best_conf = conf;
                best_class = cls;
            }
        }

        let class_name = config.class_name(best_class);
        let threshold = config.threshold_for_class(class_name);
        if best_conf < threshold {
            continue;
        }

        let cx = data[pred_idx];
        let cy = data[num_preds + pred_idx];
        let w = data[2 * num_preds + pred_idx];
        let h = data[3 * num_preds + pred_idx];

        if config.min_detection_area > 0.0 && w * h < config.min_detection_area {
            continue;
        }

        let x = cx - w / 2.0;
        let y = cy - h / 2.0;

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

    nms(&mut detections, config.nms_threshold);
    detections.truncate(config.max_detections);

    Ok(detections)
}

#[cfg(feature = "vision")]
/// Run YOLOv8-nano inference on preprocessed image tensor.
///
/// Loads the model each time — use `run_inference_with_model` for repeated calls.
pub fn run_inference(
    model_bytes: &[u8],
    tensor: &[f32],
    input_size: u32,
    config: &VisionConfig,
) -> Result<Vec<UiDetection>, String> {
    let mut session = load_model(model_bytes)?;
    run_inference_with_model(&mut session, tensor, input_size, config)
}

#[cfg(feature = "vision")]
/// Full pipeline with pre-loaded ORT session (fast path — no model reload).
pub fn detect_ui_elements_with_model(
    png_bytes: &[u8],
    session: &mut ort::session::Session,
    goal: &str,
    config: &VisionConfig,
) -> Result<VisionResult, String> {
    use std::time::Instant;

    let pre_start = Instant::now();
    let tensor = preprocess_image(png_bytes, config.input_size)?;
    let preprocess_time_ms = pre_start.elapsed().as_millis() as u64;

    let inf_start = Instant::now();
    let raw_detections = run_inference_with_model(session, &tensor, config.input_size, config)?;
    let inference_time_ms = inf_start.elapsed().as_millis() as u64;

    let raw_detection_count = raw_detections.len() as u32;
    let tree = detections_to_tree(&raw_detections, goal, "screenshot://local");

    Ok(VisionResult {
        detections: raw_detections,
        tree,
        inference_time_ms,
        preprocess_time_ms,
        raw_detection_count,
        model_version: config.model_version.clone(),
    })
}

#[cfg(feature = "vision")]
/// Full pipeline: PNG bytes -> detections -> semantic tree
///
/// Loads the model each time. For server use, prefer
/// `detect_ui_elements_with_model` with a pre-loaded session.
pub fn detect_ui_elements(
    png_bytes: &[u8],
    model_bytes: &[u8],
    goal: &str,
    config: &VisionConfig,
) -> Result<VisionResult, String> {
    let mut session = load_model(model_bytes)?;
    detect_ui_elements_with_model(png_bytes, &mut session, goal, config)
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

    #[test]
    fn test_per_class_threshold() {
        let mut config = VisionConfig::default();
        config.class_thresholds.insert("button".to_string(), 0.5);
        config.class_thresholds.insert("text".to_string(), 0.8);
        assert!(
            (config.threshold_for_class("button") - 0.5).abs() < 0.01,
            "Button borde ha per-klass-tröskel 0.5"
        );
        assert!(
            (config.threshold_for_class("text") - 0.8).abs() < 0.01,
            "Text borde ha per-klass-tröskel 0.8"
        );
        assert!(
            (config.threshold_for_class("input") - 0.25).abs() < 0.01,
            "Input utan override borde falla tillbaka till global 0.25"
        );
    }

    #[test]
    fn test_dynamic_class_labels() {
        let mut config = VisionConfig::default();
        config.class_labels = vec![
            "btn".to_string(),
            "textfield".to_string(),
            "hyperlink".to_string(),
        ];
        assert_eq!(
            config.class_name(0),
            "btn",
            "Dynamisk klass 0 borde vara 'btn'"
        );
        assert_eq!(
            config.class_name(2),
            "hyperlink",
            "Dynamisk klass 2 borde vara 'hyperlink'"
        );
        assert_eq!(
            config.class_name(99),
            "unknown",
            "Index utanför range borde ge 'unknown'"
        );
        assert_eq!(config.num_classes(), 3, "Borde ha 3 dynamiska klasser");
    }

    #[test]
    fn test_default_class_labels_uses_ui_classes() {
        let config = VisionConfig::default();
        assert_eq!(
            config.class_name(0),
            "button",
            "Default klass 0 borde vara 'button'"
        );
        assert_eq!(
            config.num_classes(),
            UI_CLASSES.len(),
            "Default borde använda UI_CLASSES"
        );
    }

    #[test]
    fn test_model_version() {
        let mut config = VisionConfig::default();
        config.model_version = "v2.1-ui-detector".to_string();
        let json = serde_json::to_string(&config).expect("Borde serialisera");
        assert!(
            json.contains("v2.1-ui-detector"),
            "Modellversion borde finnas i serialiserad config"
        );
    }

    #[test]
    fn test_vision_config_extended_serde() {
        // Testa att nya fält har korrekta defaults vid deserialisering av gammal JSON
        let old_json = r#"{"confidence_threshold":0.3,"nms_threshold":0.5,"input_size":640,"max_detections":50}"#;
        let config: VisionConfig = serde_json::from_str(old_json).expect("Borde deserialisera");
        assert!(
            config.class_thresholds.is_empty(),
            "Borde ha tom class_thresholds"
        );
        assert!(config.class_labels.is_empty(), "Borde ha tom class_labels");
        assert!(
            config.model_version.is_empty(),
            "Borde ha tom model_version"
        );
        assert!(
            (config.min_detection_area - 0.0).abs() < 0.001,
            "Borde ha 0 min_detection_area"
        );
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

    // ─── Prestandatester ────────────────────────────────────────────────────

    #[test]
    fn test_nms_performance_100_detections() {
        // NMS på 100 detektioner borde klara sig under 5ms
        let mut detections: Vec<UiDetection> = (0..100)
            .map(|i| UiDetection {
                class: UI_CLASSES[i % UI_CLASSES.len()].to_string(),
                confidence: 0.95 - (i as f32 * 0.005),
                bbox: BoundingBox {
                    x: (i % 10) as f32 * 130.0,
                    y: (i / 10) as f32 * 90.0,
                    width: 120.0,
                    height: 40.0,
                },
            })
            .collect();

        let start = std::time::Instant::now();
        nms(&mut detections, 0.45);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 5,
            "NMS på 100 detektioner tog {}ms, borde vara <5ms",
            elapsed.as_millis()
        );
        assert!(
            !detections.is_empty(),
            "NMS borde behålla åtminstone några detektioner"
        );
    }

    #[test]
    fn test_nms_performance_500_detections_heavy_overlap() {
        // Stresstest: 500 starkt överlappande detektioner
        let mut detections: Vec<UiDetection> = (0..500)
            .map(|i| UiDetection {
                class: "button".to_string(),
                confidence: 0.99 - (i as f32 * 0.001),
                bbox: BoundingBox {
                    x: 100.0 + (i as f32 * 0.5),
                    y: 100.0 + (i as f32 * 0.3),
                    width: 200.0,
                    height: 50.0,
                },
            })
            .collect();

        let start = std::time::Instant::now();
        nms(&mut detections, 0.45);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "NMS på 500 överlappande detektioner tog {}ms, borde vara <50ms",
            elapsed.as_millis()
        );
        // Starkt överlappande → borde filtreras ner kraftigt
        assert!(
            detections.len() < 20,
            "500 överlappande borde filtreras till <20, fick {}",
            detections.len()
        );
    }

    #[test]
    fn test_detections_to_tree_performance_100_elements() {
        // Bygga semantiskt träd av 100 detektioner borde ta <1ms
        let detections: Vec<UiDetection> = (0..100)
            .map(|i| UiDetection {
                class: UI_CLASSES[i % UI_CLASSES.len()].to_string(),
                confidence: 0.8,
                bbox: BoundingBox {
                    x: (i % 10) as f32 * 130.0,
                    y: (i / 10) as f32 * 90.0,
                    width: 120.0,
                    height: 40.0,
                },
            })
            .collect();

        let start = std::time::Instant::now();
        let tree = detections_to_tree(&detections, "hitta kontaktuppgifter", "https://example.com");
        let elapsed = start.elapsed();

        assert_eq!(tree.nodes.len(), 100, "Borde skapa 100 noder");
        assert!(
            elapsed.as_millis() < 1,
            "detections_to_tree för 100 element tog {}ms, borde vara <1ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_vision_config_per_class_threshold_filtering() {
        // Simulera hjo.se-scenariot: höj threshold för select/input för att filtrera FP
        let mut config = VisionConfig::default();
        config.class_thresholds.insert("select".to_string(), 0.6);
        config.class_thresholds.insert("input".to_string(), 0.6);
        config.class_thresholds.insert("button".to_string(), 0.3);

        // select med 42% confidence borde filtreras (under 60%)
        assert!(
            config.threshold_for_class("select") > 0.42,
            "select-tröskeln (0.6) borde filtrera 42% confidence"
        );
        // button med 98% confidence borde behållas (över 30%)
        assert!(
            config.threshold_for_class("button") < 0.98,
            "button-tröskeln (0.3) borde behålla 98% confidence"
        );
        // input med 37% confidence borde filtreras (under 60%)
        assert!(
            config.threshold_for_class("input") > 0.37,
            "input-tröskeln (0.6) borde filtrera 37% confidence"
        );
    }

    #[test]
    fn test_min_detection_area_filters_artefacts() {
        let config = VisionConfig {
            min_detection_area: 500.0,
            ..VisionConfig::default()
        };
        // Liten artefakt: 10×10 = 100px² < 500 → borde filtreras
        assert!(
            10.0 * 10.0 < config.min_detection_area,
            "10×10 detektion borde filtreras av min_detection_area=500"
        );
        // Normal knapp: 120×40 = 4800px² > 500 → borde behållas
        assert!(
            120.0 * 40.0 > config.min_detection_area,
            "120×40 detektion borde passera min_detection_area=500"
        );
    }

    #[test]
    fn test_detections_to_tree_goal_relevance_scoring() {
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
                class: "text".to_string(),
                confidence: 0.88,
                bbox: BoundingBox {
                    x: 50.0,
                    y: 100.0,
                    width: 200.0,
                    height: 25.0,
                },
            },
        ];

        // Mål som nämner "button" borde ge button högre relevans
        let tree_button_goal =
            detections_to_tree(&detections, "click the button", "https://example.com");
        let tree_text_goal =
            detections_to_tree(&detections, "read the text", "https://example.com");

        let button_rel_with_button_goal = tree_button_goal.nodes[0].relevance;
        let button_rel_with_text_goal = tree_text_goal.nodes[0].relevance;
        assert!(
            button_rel_with_button_goal > button_rel_with_text_goal,
            "Button borde ha högre relevans med 'click the button' ({}) vs 'read the text' ({})",
            button_rel_with_button_goal,
            button_rel_with_text_goal
        );

        let text_rel_with_text_goal = tree_text_goal.nodes[1].relevance;
        let text_rel_with_button_goal = tree_button_goal.nodes[1].relevance;
        assert!(
            text_rel_with_text_goal > text_rel_with_button_goal,
            "Text borde ha högre relevans med 'read the text' ({}) vs 'click the button' ({})",
            text_rel_with_text_goal,
            text_rel_with_button_goal
        );
    }

    #[test]
    fn test_detections_to_tree_all_classes_mapped() {
        // Verifiera att alla UI_CLASSES mappar till giltiga roller
        let detections: Vec<UiDetection> = UI_CLASSES
            .iter()
            .enumerate()
            .map(|(i, cls)| UiDetection {
                class: cls.to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: i as f32 * 100.0,
                    y: 0.0,
                    width: 80.0,
                    height: 30.0,
                },
            })
            .collect();

        let tree = detections_to_tree(&detections, "test", "url");
        assert_eq!(
            tree.nodes.len(),
            UI_CLASSES.len(),
            "Borde skapa en nod per UI-klass"
        );

        for node in &tree.nodes {
            assert!(
                !node.role.is_empty(),
                "Nod-roll borde inte vara tom för klass"
            );
            assert_eq!(
                node.trust,
                TrustLevel::Untrusted,
                "Alla vision-noder borde vara Untrusted"
            );
            assert!(node.bbox.is_some(), "Alla vision-noder borde ha bbox");
        }
    }

    #[test]
    fn test_vision_result_serde_roundtrip() {
        let result = VisionResult {
            detections: vec![UiDetection {
                class: "button".to_string(),
                confidence: 0.95,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 20.0,
                    width: 100.0,
                    height: 40.0,
                },
            }],
            tree: detections_to_tree(
                &[UiDetection {
                    class: "button".to_string(),
                    confidence: 0.95,
                    bbox: BoundingBox {
                        x: 10.0,
                        y: 20.0,
                        width: 100.0,
                        height: 40.0,
                    },
                }],
                "test",
                "url",
            ),
            inference_time_ms: 171,
            preprocess_time_ms: 185,
            raw_detection_count: 12,
            model_version: "v1-int8".to_string(),
        };

        let json = serde_json::to_string(&result).expect("Borde kunna serialisera VisionResult");
        let parsed: VisionResult =
            serde_json::from_str(&json).expect("Borde kunna deserialisera VisionResult");

        assert_eq!(
            parsed.detections.len(),
            1,
            "Borde ha 1 detektion efter roundtrip"
        );
        assert_eq!(
            parsed.inference_time_ms, 171,
            "inference_time_ms borde överleva roundtrip"
        );
        assert_eq!(
            parsed.preprocess_time_ms, 185,
            "preprocess_time_ms borde överleva roundtrip"
        );
        assert_eq!(
            parsed.raw_detection_count, 12,
            "raw_detection_count borde överleva roundtrip"
        );
        assert_eq!(
            parsed.model_version, "v1-int8",
            "model_version borde överleva roundtrip"
        );
    }

    #[test]
    fn test_nms_preserves_sort_order() {
        // NMS borde returnera detektioner sorterade efter confidence (högst först)
        let mut detections = vec![
            UiDetection {
                class: "button".to_string(),
                confidence: 0.5,
                bbox: BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    width: 50.0,
                    height: 30.0,
                },
            },
            UiDetection {
                class: "input".to_string(),
                confidence: 0.9,
                bbox: BoundingBox {
                    x: 200.0,
                    y: 200.0,
                    width: 100.0,
                    height: 30.0,
                },
            },
            UiDetection {
                class: "link".to_string(),
                confidence: 0.7,
                bbox: BoundingBox {
                    x: 400.0,
                    y: 0.0,
                    width: 60.0,
                    height: 20.0,
                },
            },
        ];
        nms(&mut detections, 0.45);
        assert_eq!(detections.len(), 3, "Icke-överlappande borde alla behållas");
        assert!(
            detections[0].confidence >= detections[1].confidence
                && detections[1].confidence >= detections[2].confidence,
            "Detektioner borde vara sorterade efter confidence (högst först): {}, {}, {}",
            detections[0].confidence,
            detections[1].confidence,
            detections[2].confidence
        );
    }
}
