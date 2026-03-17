/// Multimodal Grounding – Fas 9c
///
/// Accepterar bounding-box-koordinater och berikar det semantiska
/// trädet med spatial data. Möjliggör Set-of-Mark-integration
/// och mappning mellan visuella koordinater och DOM-element.
///
/// Pipeline:
/// 1. Ta emot bbox-array (från getBoundingClientRect eller vision-modell)
/// 2. Matcha varje bbox till en nod via id/label/role
/// 3. Berika SemanticNode med spatial information
/// 4. Generera Set-of-Mark-annotationer
use serde::{Deserialize, Serialize};

use crate::types::{BoundingBox, SemanticNode, SemanticTree};

// ─── Types ──────────────────────────────────────────────────────────────────

/// En bbox-annotation att matcha mot en nod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BboxAnnotation {
    /// Matchningskriterium: html_id, name, eller label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_id: Option<String>,
    /// Roll att matcha mot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Label att matcha mot (fuzzy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Bounding box
    pub bbox: BoundingBox,
}

/// Resultat av grounding-processen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingResult {
    /// Berikat semantiskt träd (med bbox-data)
    pub tree: SemanticTree,
    /// Antal noder som matchades med bbox
    pub matched_count: u32,
    /// Antal bbox:ar som inte matchade någon nod
    pub unmatched_count: u32,
    /// Set-of-Mark-annotationer (nod-ID → markör-nummer)
    pub set_of_marks: Vec<SetOfMark>,
    /// Grounding-tid i ms
    pub grounding_time_ms: u64,
}

/// Set-of-Mark: mappar visuella markörer till noder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetOfMark {
    /// Markör-nummer (1, 2, 3, ...)
    pub mark: u32,
    /// Nod-ID
    pub node_id: u32,
    /// Roll
    pub role: String,
    /// Label
    pub label: String,
    /// Bbox-koordinater
    pub bbox: BoundingBox,
}

/// IoU (Intersection over Union) mellan två bounding boxes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoUMatch {
    /// Nod-ID
    pub node_id: u32,
    /// IoU-värde (0.0–1.0)
    pub iou: f32,
    /// Roll
    pub role: String,
    /// Label
    pub label: String,
}

// ─── Implementation ─────────────────────────────────────────────────────────

/// Berika ett semantiskt träd med bounding boxes
pub fn ground_tree(tree: &SemanticTree, annotations: &[BboxAnnotation]) -> GroundingResult {
    let mut grounded_tree = tree.clone();
    let mut matched = 0u32;
    let mut unmatched = 0u32;

    for annotation in annotations {
        let matched_node = find_matching_node_mut(&mut grounded_tree.nodes, annotation);
        if matched_node {
            matched += 1;
        } else {
            unmatched += 1;
        }
    }

    // Generera Set-of-Mark
    let set_of_marks = generate_set_of_marks(&grounded_tree.nodes);

    GroundingResult {
        tree: grounded_tree,
        matched_count: matched,
        unmatched_count: unmatched,
        set_of_marks,
        grounding_time_ms: 0,
    }
}

/// Hitta och uppdatera matchande nod (rekursivt)
fn find_matching_node_mut(nodes: &mut [SemanticNode], annotation: &BboxAnnotation) -> bool {
    for node in nodes.iter_mut() {
        if matches_annotation(node, annotation) {
            node.bbox = Some(annotation.bbox.clone());
            return true;
        }
        if find_matching_node_mut(&mut node.children, annotation) {
            return true;
        }
    }
    false
}

/// Kontrollera om en nod matchar en annotation
fn matches_annotation(node: &SemanticNode, annotation: &BboxAnnotation) -> bool {
    // Matcha via html_id (exakt)
    if let Some(ref target_id) = annotation.html_id {
        if let Some(ref node_id) = node.html_id {
            if node_id == target_id {
                return true;
            }
        }
    }

    // Matcha via roll + label (fuzzy)
    let role_matches = annotation
        .role
        .as_ref()
        .map(|r| r == &node.role)
        .unwrap_or(true);

    let label_matches = annotation
        .label
        .as_ref()
        .map(|l| {
            let l_lower = l.to_lowercase();
            let n_lower = node.label.to_lowercase();
            n_lower.contains(&l_lower) || l_lower.contains(&n_lower)
        })
        .unwrap_or(false);

    role_matches && label_matches && annotation.html_id.is_none()
}

/// Generera Set-of-Mark-annotationer för interaktiva noder med bbox
fn generate_set_of_marks(nodes: &[SemanticNode]) -> Vec<SetOfMark> {
    let mut marks = Vec::new();
    let mut mark_counter = 1u32;
    collect_marks(nodes, &mut marks, &mut mark_counter);
    marks
}

/// Rekursiv insamling av markörer
fn collect_marks(nodes: &[SemanticNode], marks: &mut Vec<SetOfMark>, counter: &mut u32) {
    for node in nodes {
        if let Some(ref bbox) = node.bbox {
            // Markera bara interaktiva element
            if node.action.is_some() {
                marks.push(SetOfMark {
                    mark: *counter,
                    node_id: node.id,
                    role: node.role.clone(),
                    label: node.label.clone(),
                    bbox: bbox.clone(),
                });
                *counter += 1;
            }
        }
        collect_marks(&node.children, marks, counter);
    }
}

/// Beräkna IoU mellan två bounding boxes
pub fn compute_iou(a: &BoundingBox, b: &BoundingBox) -> f32 {
    let x_overlap = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let y_overlap = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);

    if x_overlap <= 0.0 || y_overlap <= 0.0 {
        return 0.0;
    }

    let intersection = x_overlap * y_overlap;
    let area_a = a.width * a.height;
    let area_b = b.width * b.height;
    let union = area_a + area_b - intersection;

    if union <= 0.0 {
        return 0.0;
    }

    intersection / union
}

/// Matcha en predikterad bbox mot alla noder med bbox via IoU
pub fn match_by_iou(nodes: &[SemanticNode], predicted_bbox: &BoundingBox) -> Vec<IoUMatch> {
    let mut matches = Vec::new();
    collect_iou_matches(nodes, predicted_bbox, &mut matches);
    matches.sort_by(|a, b| b.iou.total_cmp(&a.iou));
    matches
}

/// Rekursiv IoU-matchning
fn collect_iou_matches(
    nodes: &[SemanticNode],
    predicted: &BoundingBox,
    matches: &mut Vec<IoUMatch>,
) {
    for node in nodes {
        if let Some(ref node_bbox) = node.bbox {
            let iou = compute_iou(node_bbox, predicted);
            if iou > 0.0 {
                matches.push(IoUMatch {
                    node_id: node.id,
                    iou,
                    role: node.role.clone(),
                    label: node.label.clone(),
                });
            }
        }
        collect_iou_matches(&node.children, predicted, matches);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SemanticTree;

    fn make_node(id: u32, role: &str, label: &str, html_id: Option<&str>) -> SemanticNode {
        let mut node = SemanticNode::new(id, role, label);
        node.html_id = html_id.map(|s| s.to_string());
        node.action = SemanticNode::infer_action(role);
        node
    }

    fn make_tree(nodes: Vec<SemanticNode>) -> SemanticTree {
        SemanticTree {
            url: "https://shop.se".to_string(),
            title: "Test".to_string(),
            goal: "köp".to_string(),
            nodes,
            injection_warnings: vec![],
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
        }
    }

    #[test]
    fn test_ground_tree_by_id() {
        let tree = make_tree(vec![make_node(1, "button", "Köp", Some("buy-btn"))]);
        let annotations = vec![BboxAnnotation {
            html_id: Some("buy-btn".to_string()),
            role: None,
            label: None,
            bbox: BoundingBox {
                x: 100.0,
                y: 200.0,
                width: 80.0,
                height: 30.0,
            },
        }];

        let result = ground_tree(&tree, &annotations);
        assert_eq!(result.matched_count, 1, "Borde matcha 1 nod via ID");
        assert_eq!(result.unmatched_count, 0);
        assert!(result.tree.nodes[0].bbox.is_some(), "Noden borde ha bbox");
    }

    #[test]
    fn test_ground_tree_by_label() {
        let tree = make_tree(vec![make_node(1, "button", "Köp nu", None)]);
        let annotations = vec![BboxAnnotation {
            html_id: None,
            role: Some("button".to_string()),
            label: Some("Köp".to_string()),
            bbox: BoundingBox {
                x: 50.0,
                y: 100.0,
                width: 120.0,
                height: 40.0,
            },
        }];

        let result = ground_tree(&tree, &annotations);
        assert_eq!(result.matched_count, 1, "Borde matcha via fuzzy label");
    }

    #[test]
    fn test_ground_tree_unmatched() {
        let tree = make_tree(vec![make_node(1, "button", "Köp", None)]);
        let annotations = vec![BboxAnnotation {
            html_id: Some("nonexistent".to_string()),
            role: None,
            label: None,
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        }];

        let result = ground_tree(&tree, &annotations);
        assert_eq!(result.unmatched_count, 1, "Borde rapportera 1 omatchad");
    }

    #[test]
    fn test_set_of_marks() {
        let tree = make_tree(vec![
            make_node(1, "button", "Köp", Some("b1")),
            make_node(2, "link", "Info", Some("l1")),
            make_node(3, "text", "Statisk text", Some("t1")),
        ]);
        let annotations = vec![
            BboxAnnotation {
                html_id: Some("b1".to_string()),
                role: None,
                label: None,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 20.0,
                    width: 100.0,
                    height: 30.0,
                },
            },
            BboxAnnotation {
                html_id: Some("l1".to_string()),
                role: None,
                label: None,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 60.0,
                    width: 80.0,
                    height: 20.0,
                },
            },
            BboxAnnotation {
                html_id: Some("t1".to_string()),
                role: None,
                label: None,
                bbox: BoundingBox {
                    x: 10.0,
                    y: 100.0,
                    width: 200.0,
                    height: 15.0,
                },
            },
        ];

        let result = ground_tree(&tree, &annotations);
        // Bara button och link har action → borde ge 2 marks
        assert_eq!(
            result.set_of_marks.len(),
            2,
            "Bara interaktiva element borde få marks"
        );
        assert_eq!(result.set_of_marks[0].mark, 1);
        assert_eq!(result.set_of_marks[1].mark, 2);
    }

    #[test]
    fn test_compute_iou_full_overlap() {
        let a = BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let iou = compute_iou(&a, &a);
        assert!(
            (iou - 1.0).abs() < 0.001,
            "Full överlappning borde ge IoU ≈ 1.0"
        );
    }

    #[test]
    fn test_compute_iou_no_overlap() {
        let a = BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        };
        let b = BoundingBox {
            x: 100.0,
            y: 100.0,
            width: 50.0,
            height: 50.0,
        };
        let iou = compute_iou(&a, &b);
        assert_eq!(iou, 0.0, "Ingen överlappning borde ge IoU = 0");
    }

    #[test]
    fn test_compute_iou_partial() {
        let a = BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let b = BoundingBox {
            x: 50.0,
            y: 50.0,
            width: 100.0,
            height: 100.0,
        };
        let iou = compute_iou(&a, &b);
        // Intersection: 50×50 = 2500, Union: 10000+10000-2500 = 17500
        let expected = 2500.0 / 17500.0;
        assert!(
            (iou - expected).abs() < 0.01,
            "Partiell överlappning: förväntade {:.3}, fick {:.3}",
            expected,
            iou
        );
    }

    #[test]
    fn test_match_by_iou() {
        let mut nodes = vec![make_node(1, "button", "Köp", Some("b1"))];
        nodes[0].bbox = Some(BoundingBox {
            x: 100.0,
            y: 200.0,
            width: 80.0,
            height: 30.0,
        });

        let predicted = BoundingBox {
            x: 105.0,
            y: 205.0,
            width: 70.0,
            height: 25.0,
        };

        let matches = match_by_iou(&nodes, &predicted);
        assert!(!matches.is_empty(), "Borde hitta matchande nod");
        assert!(matches[0].iou > 0.5, "IoU borde vara hög vid nära match");
    }

    #[test]
    fn test_grounding_result_serialization() {
        let tree = make_tree(vec![]);
        let result = GroundingResult {
            tree,
            matched_count: 0,
            unmatched_count: 0,
            set_of_marks: vec![],
            grounding_time_ms: 1,
        };
        let json = serde_json::to_string(&result).expect("Borde serialisera");
        assert!(json.contains("matched_count"));
    }
}
