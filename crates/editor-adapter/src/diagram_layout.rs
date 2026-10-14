//! Keep layout engine selection independent of cached SVGs and GPUI windows.
use mermaid_rs_renderer::{
    DiagramKind, LayoutConfig, Theme, compute_layout,
    ir::{Direction, EdgeStyleOverride, Graph, Node, NodeShape},
    layout::{EdgeLayout, Layout, TextBlock},
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Backend {
    Native,
    Elk,
}

pub(crate) fn layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
    init: Option<&Value>,
) -> (Layout, Backend) {
    let requested = init
        .and_then(|init| init.pointer("/flowchart/defaultRenderer"))
        .and_then(Value::as_str);
    if graph.kind == DiagramKind::Flowchart
        && !matches!(requested, Some("dagre" | "dagre-d3" | "dagre-wrapper"))
    {
        // Each worker owns its ELK graph and registries. No global engine lock
        // serializes the five shared diagram slots, and user input is JSON data.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            elk_layout(graph, theme, config, init)
        }));
        if let Ok(Ok(layout)) = result {
            return (layout, Backend::Elk);
        }
    }
    // Preserve rendering for unsupported kinds and layouts the port cannot handle.
    (compute_layout(graph, theme, config), Backend::Native)
}

fn elk_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
    init: Option<&Value>,
) -> Result<Layout, String> {
    // Removing edges avoids the expensive native routing/label-placement passes.
    // Its node sizing still supplies the existing wrapping, shapes, styles, links
    // and group headers, so adopting ELK does not invent another text measurer.
    let mut seed = graph.clone();
    seed.edges.clear();
    let mut layout = compute_layout(&seed, theme, config);
    let groups = layout.subgraphs.len();
    let parents = (0..groups)
        .map(|index| {
            let nodes = &layout.subgraphs[index].nodes;
            (0..groups)
                .filter(|parent| {
                    *parent != index
                        && layout.subgraphs[*parent].nodes.len() > nodes.len()
                        && !nodes.is_empty()
                        && nodes
                            .iter()
                            .all(|node| layout.subgraphs[*parent].nodes.contains(node))
                })
                .min_by_key(|parent| layout.subgraphs[*parent].nodes.len())
        })
        .collect::<Vec<_>>();
    let mut aliases = HashMap::new();
    for group in &graph.subgraphs {
        if let Some(id) = &group.id
            && let Some(index) = layout
                .subgraphs
                .iter()
                .position(|seed| seed.label == group.label && seed.nodes == group.nodes)
        {
            aliases.insert(id.clone(), index);
        }
    }
    let mut nodes = layout
        .nodes
        .keys()
        .filter(|id| !aliases.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    nodes.sort_by_key(|id| graph.node_order.get(id).copied().unwrap_or(usize::MAX));
    let ids = nodes
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), format!("n{index}")))
        .chain(
            aliases
                .iter()
                .map(|(id, index)| (id.clone(), format!("g{index}"))),
        )
        .collect::<HashMap<_, _>>();
    let owners = nodes
        .iter()
        .map(|id| {
            (
                id.clone(),
                (0..groups)
                    .filter(|group| layout.subgraphs[*group].nodes.contains(id))
                    .min_by_key(|group| layout.subgraphs[*group].nodes.len()),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut options = json!({
        "elk.algorithm": "layered", "elk.direction": direction_name(graph.direction),
        "elk.hierarchyHandling": "INCLUDE_CHILDREN", "elk.edgeRouting": "ORTHOGONAL",
        "elk.spacing.nodeNode": config.node_spacing,
        "elk.layered.spacing.nodeNodeBetweenLayers": config.rank_spacing,
        "elk.padding": "[top=24,left=24,bottom=24,right=24]",
        "elk.randomSeed": 1
    });
    if let Some(strategy) = init
        .and_then(|init| init.pointer("/elk/nodePlacementStrategy"))
        .and_then(Value::as_str)
        && matches!(
            strategy,
            "SIMPLE" | "NETWORK_SIMPLEX" | "BRANDES_KOEPF" | "LINEAR_SEGMENTS"
        )
    {
        options["elk.layered.nodePlacement.strategy"] = json!(strategy);
    }
    if let Some(force) = init
        .and_then(|init| init.pointer("/elk/forceNodeModelOrder"))
        .and_then(Value::as_bool)
    {
        options["elk.layered.crossingMinimization.forceNodeModelOrder"] = json!(force);
        options["elk.layered.considerModelOrder.strategy"] = json!("NODES_AND_EDGES");
    }
    let group_options = layout
        .subgraphs
        .iter()
        .map(|group| {
            let mut options = options.clone();
            options["elk.padding"] = json!(format!(
                "[top={},left=30,bottom=30,right=30]",
                group.label_block.height + 30.0
            ));
            options["elk.nodeSize.minimum"] = json!(format!(
                "({},{})",
                group.label_block.width + 60.0,
                group.label_block.height + 60.0
            ));
            options["elk.nodeSize.constraints"] = json!("MINIMUM_SIZE");
            let direction = graph
                .subgraphs
                .iter()
                .find(|source| source.label == group.label && source.nodes == group.nodes)
                .and_then(|source| source.direction);
            // Mermaid ignores a subgraph direction when one of its members is
            // connected outside. Group-to-group links do not cross that boundary.
            let crosses_boundary = graph
                .edges
                .iter()
                .any(|edge| group.nodes.contains(&edge.from) != group.nodes.contains(&edge.to));
            if let Some(direction) = direction
                && !crosses_boundary
            {
                options["elk.direction"] = json!(direction_name(direction));
                // Merged layered hierarchies inherit the root direction. Lay
                // out this isolated group separately to preserve its override.
                options["elk.hierarchyHandling"] = json!("SEPARATE_CHILDREN");
            }
            options
        })
        .collect::<Vec<_>>();
    fn children(
        parent: Option<usize>,
        layout: &Layout,
        nodes: &[String],
        owners: &HashMap<String, Option<usize>>,
        parents: &[Option<usize>],
        ids: &HashMap<String, String>,
        group_options: &[Value],
    ) -> Vec<Value> {
        let mut result = nodes
            .iter()
            .filter(|id| owners[*id] == parent)
            .map(|id| {
                let node = &layout.nodes[id];
                json!({ "id": ids[id], "width": node.width, "height": node.height,
                    "layoutOptions": {"elk.nodeSize.constraints": "[]"} })
            })
            .collect::<Vec<_>>();
        for (index, _) in parents
            .iter()
            .enumerate()
            .filter(|(_, owner)| **owner == parent)
        {
            result.push(
                json!({"id": format!("g{index}"), "layoutOptions": group_options[index],
                "children": children(Some(index), layout, nodes, owners, parents, ids, group_options)}),
            );
        }
        result
    }
    let label_blocks = edge_labels(graph, theme, config);
    let mut edge_layouts = Vec::new();
    let mut edges = Vec::new();
    for (index, edge) in graph.edges.iter().enumerate() {
        let mut labels = Vec::new();
        for (name, placement, text) in [
            ("center", "CENTER", &edge.label),
            ("start", "TAIL", &edge.start_label),
            ("end", "HEAD", &edge.end_label),
        ] {
            if let Some(text) = text {
                let block = &label_blocks[text];
                labels.push(json!({"id": name, "text": text, "width": block.width + 8.0, "height": block.height + 8.0,
                    "layoutOptions": {"elk.edgeLabels.placement": placement}}));
            }
        }
        let from = ids.get(&edge.from).ok_or("missing ELK source")?;
        let to = ids.get(&edge.to).ok_or("missing ELK target")?;
        edges.push(json!({"id": format!("e{index}"), "sources": [from], "targets": [to], "labels": labels}));
        let mut style = graph.edge_style_default.clone().unwrap_or_default();
        if let Some(overrides) = graph.edge_styles.get(&index) {
            merge_edge_style(&mut style, overrides);
        }
        edge_layouts.push(EdgeLayout {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: edge.label.as_ref().map(|text| label_blocks[text].clone()),
            start_label: edge
                .start_label
                .as_ref()
                .map(|text| label_blocks[text].clone()),
            end_label: edge
                .end_label
                .as_ref()
                .map(|text| label_blocks[text].clone()),
            label_anchor: None,
            start_label_anchor: None,
            end_label_anchor: None,
            points: Vec::new(),
            directed: edge.directed,
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
            arrow_start_kind: edge.arrow_start_kind,
            arrow_end_kind: edge.arrow_end_kind,
            start_decoration: edge.start_decoration,
            end_decoration: edge.end_decoration,
            style: edge.style,
            override_style: style,
        });
    }
    let input = json!({"id": "root", "layoutOptions": options,
        "children": children(None, &layout, &nodes, &owners, &parents, &ids, &group_options), "edges": edges});
    let output = elkrs::create_elk().layout_json(&input.to_string())?;
    layout.edges = edge_layouts;
    let mut seen = HashSet::new();
    apply_output(&output, (0.0, 0.0), &nodes, &mut layout, &mut seen)?;
    if seen.len() != nodes.len() + groups || layout.edges.iter().any(|edge| edge.points.len() < 2) {
        return Err("incomplete ELK layout".into());
    }
    for (id, index) in aliases {
        if let Some(node) = layout.nodes.get_mut(&id) {
            let group = &layout.subgraphs[index];
            node.x = group.x;
            node.y = group.y;
            node.width = group.width;
            node.height = group.height;
            node.hidden = true;
            node.anchor_subgraph = Some(index);
        }
    }
    for edge in &mut layout.edges {
        if let Some(node) = layout.nodes.get(&edge.from) {
            edge.points[0] = clip_endpoint(node, edge.points[0]);
        }
        if let Some(node) = layout.nodes.get(&edge.to) {
            let last = edge.points.len() - 1;
            edge.points[last] = clip_endpoint(node, edge.points[last]);
        }
    }
    layout.width = number(&output, "width")?;
    layout.height = number(&output, "height")?;
    Ok(layout)
}

fn clip_endpoint(node: &mermaid_rs_renderer::layout::NodeLayout, point: (f32, f32)) -> (f32, f32) {
    // ELK routes to bounding rectangles. Project only the terminal point onto
    // the painted convex outline so multi-port arrows do not float beside a
    // diamond, circle, rounded box or database cylinder.
    let (hw, hh) = (node.width / 2.0, node.height / 2.0);
    let (cx, cy) = (node.x + hw, node.y + hh);
    let (dx, dy) = (point.0 - cx, point.1 - cy);
    let inside = |t: f32| {
        let (x, y) = (dx * t, dy * t);
        let (nx, ny) = (x / hw, y / hh);
        match node.shape {
            NodeShape::Diamond => nx.abs() + ny.abs() <= 1.0,
            NodeShape::Circle | NodeShape::DoubleCircle => nx * nx + ny * ny <= 1.0,
            NodeShape::Hexagon => nx.abs() + ny.abs() / 2.0 <= 1.0,
            NodeShape::Parallelogram => {
                nx >= -1.0 + 0.18 * (1.0 - ny) && nx <= 1.0 - 0.18 * (1.0 + ny)
            }
            NodeShape::ParallelogramAlt => {
                -nx >= -1.0 + 0.18 * (1.0 - ny) && -nx <= 1.0 - 0.18 * (1.0 + ny)
            }
            NodeShape::Trapezoid => nx.abs() <= 0.82 + 0.18 * ny,
            NodeShape::TrapezoidAlt => nx.abs() <= 0.82 - 0.18 * ny,
            NodeShape::Asymmetric => nx <= 1.0 - 0.44 * ny.abs(),
            NodeShape::Cylinder => {
                let radius = (node.height * 0.12).clamp(6.0, 14.0);
                let cap = (y.abs() - (hh - radius)).max(0.0) / radius;
                nx * nx + cap * cap <= 1.0
            }
            NodeShape::RoundRect | NodeShape::Stadium => {
                let radius = if node.shape == NodeShape::Stadium {
                    hh.min(hw)
                } else {
                    10.0_f32.min(hh).min(hw)
                };
                let x = (x.abs() - hw + radius).max(0.0);
                let y = (y.abs() - hh + radius).max(0.0);
                x * x + y * y <= radius * radius
            }
            _ => true,
        }
    };
    if inside(1.0) {
        return point;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if inside(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    (cx + dx * low, cy + dy * low)
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::LeftRight => "RIGHT",
        Direction::RightLeft => "LEFT",
        Direction::TopDown => "DOWN",
        Direction::BottomTop => "UP",
    }
}

fn edge_labels(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> HashMap<String, TextBlock> {
    let mut seed = graph.clone();
    seed.nodes.clear();
    seed.node_order.clear();
    seed.subgraphs.clear();
    seed.edges.clear();
    seed.node_styles.clear();
    seed.node_classes.clear();
    seed.node_links.clear();
    let mut labels = HashMap::new();
    for text in graph
        .edges
        .iter()
        .flat_map(|edge| [&edge.label, &edge.start_label, &edge.end_label])
        .flatten()
    {
        if !labels.contains_key(text) {
            let id = format!("label{}", labels.len());
            seed.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    label: text.clone(),
                    shape: NodeShape::Rectangle,
                    value: None,
                    icon: None,
                },
            );
            labels.insert(text.clone(), id);
        }
    }
    if labels.is_empty() {
        return HashMap::new();
    }
    let layout = compute_layout(&seed, theme, config);
    labels
        .into_iter()
        .map(|(text, id)| (text, layout.nodes[&id].label.clone()))
        .collect()
}

fn number(value: &Value, key: &str) -> Result<f32, String> {
    let result = value[key]
        .as_f64()
        .ok_or_else(|| format!("missing ELK {key}"))? as f32;
    if !result.is_finite() || result.abs() > 128_000.0 {
        return Err("invalid ELK coordinate".into());
    }
    Ok(result)
}

fn apply_output(
    value: &Value,
    origin: (f32, f32),
    nodes: &[String],
    layout: &mut Layout,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    if let Some(children) = value["children"].as_array() {
        for child in children {
            let id = child["id"].as_str().ok_or("missing ELK node id")?;
            let x = origin.0 + number(child, "x")?;
            let y = origin.1 + number(child, "y")?;
            let width = number(child, "width")?;
            let height = number(child, "height")?;
            if width <= 0.0 || height <= 0.0 || !seen.insert(id.to_owned()) {
                return Err("invalid ELK box".into());
            }
            if let Some(index) = id.strip_prefix('n').and_then(|id| id.parse::<usize>().ok()) {
                let node = layout
                    .nodes
                    .get_mut(nodes.get(index).ok_or("unknown ELK node")?)
                    .ok_or("missing seed node")?;
                node.x = x;
                node.y = y;
            } else if let Some(index) = id.strip_prefix('g').and_then(|id| id.parse::<usize>().ok())
            {
                let group = layout.subgraphs.get_mut(index).ok_or("unknown ELK group")?;
                group.x = x;
                group.y = y;
                group.width = width;
                group.height = height;
            } else {
                return Err("unknown ELK node id".into());
            }
            apply_output(child, (x, y), nodes, layout, seen)?;
        }
    }
    if let Some(edges) = value["edges"].as_array() {
        for edge in edges {
            let index = edge["id"]
                .as_str()
                .and_then(|id| id.strip_prefix('e'))
                .and_then(|id| id.parse::<usize>().ok())
                .ok_or("invalid ELK edge id")?;
            let target = layout.edges.get_mut(index).ok_or("unknown ELK edge")?;
            for section in edge["sections"].as_array().ok_or("missing ELK sections")? {
                let points = std::iter::once(&section["startPoint"])
                    .chain(section["bendPoints"].as_array().into_iter().flatten())
                    .chain(std::iter::once(&section["endPoint"]));
                for point in points {
                    let point = (
                        origin.0 + number(point, "x")?,
                        origin.1 + number(point, "y")?,
                    );
                    if target.points.last() != Some(&point) {
                        target.points.push(point);
                    }
                }
            }
            if let Some(labels) = edge["labels"].as_array() {
                for label in labels {
                    let anchor = Some((
                        origin.0 + number(label, "x")? + number(label, "width")? / 2.0,
                        origin.1 + number(label, "y")? + number(label, "height")? / 2.0,
                    ));
                    match label["id"].as_str() {
                        Some("center") => target.label_anchor = anchor,
                        Some("start") => target.start_label_anchor = anchor,
                        Some("end") => target.end_label_anchor = anchor,
                        _ => return Err("unknown ELK label".into()),
                    }
                }
            }
        }
    }
    Ok(())
}

fn merge_edge_style(style: &mut EdgeStyleOverride, overrides: &EdgeStyleOverride) {
    if overrides.stroke.is_some() {
        style.stroke = overrides.stroke.clone();
    }
    if overrides.stroke_width.is_some() {
        style.stroke_width = overrides.stroke_width;
    }
    if overrides.dasharray.is_some() {
        style.dasharray = overrides.dasharray.clone();
    }
    if overrides.label_color.is_some() {
        style.label_color = overrides.label_color.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::parse_mermaid_strict;

    #[test]
    fn isolated_subgraph_retains_its_explicit_direction() {
        let parsed = parse_mermaid_strict(
            "flowchart TD\nsubgraph G[阶段]\ndirection LR\nA --> B\nend\nX --> Y",
        )
        .unwrap();
        let output = elk_layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        )
        .unwrap();
        assert!(output.nodes["B"].x > output.nodes["A"].x + output.nodes["A"].width);
        assert!((output.nodes["B"].y - output.nodes["A"].y).abs() < 0.01);
        assert!(output.nodes["Y"].y > output.nodes["X"].y);
    }

    #[test]
    fn edge_terminals_match_the_svg_nodes_top_left_coordinates() {
        let parsed = parse_mermaid_strict("flowchart TD\nA[开始] --> B[完成]").unwrap();
        let output = elk_layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        )
        .unwrap();
        let (a, b) = (&output.nodes["A"], &output.nodes["B"]);
        let (start, end) = (
            output.edges[0].points[0],
            *output.edges[0].points.last().unwrap(),
        );
        assert!((start.0 - a.x - a.width / 2.0).abs() < 0.01);
        assert!((start.1 - a.y - a.height).abs() < 0.01);
        assert!((end.0 - b.x - b.width / 2.0).abs() < 0.01);
        assert!((end.1 - b.y).abs() < 0.01);
    }

    #[test]
    fn flowcharts_use_elk_and_retain_every_node_edge_and_label() {
        let parsed = parse_mermaid_strict(
            "flowchart LR\nA[开始] -->|下一步| B{保存？}\nB -->|是| C[完成]\nB -->|否| A",
        )
        .unwrap();
        let (layout, backend) = layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            parsed.init_config.as_ref(),
        );
        assert_eq!(
            backend,
            Backend::Elk,
            "flowcharts must actually use the ELK layout engine"
        );
        assert_eq!(layout.nodes.len(), parsed.graph.nodes.len());
        assert_eq!(layout.edges.len(), parsed.graph.edges.len());
        assert!(
            layout
                .nodes
                .values()
                .all(|node| node.x.is_finite() && node.y.is_finite())
        );
        assert!(layout.edges.iter().all(|edge| edge.points.len() >= 2));
        assert_eq!(
            layout.edges[0].label.as_ref().unwrap().lines.join(""),
            "下一步"
        );
        assert!(layout.width > 0.0 && layout.height > 0.0);
    }

    #[test]
    fn nested_groups_and_cross_group_edges_keep_absolute_coordinates() {
        let parsed = parse_mermaid_strict("flowchart TD\nsubgraph Outer[外部]\nsubgraph Inner[内部]\nA[开始] --> B[处理]\nend\nB --> C[继续]\nend\nC --> D[完成]").unwrap();
        let output = elk_layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        )
        .unwrap();
        assert_eq!(output.subgraphs.len(), 2);
        for group in &output.subgraphs {
            for id in &group.nodes {
                if let Some(node) = output.nodes.get(id) {
                    assert!(node.x >= group.x - 0.01, "{id} outside {}", group.label);
                    assert!(
                        node.y >= group.y + group.label_block.height + 12.0 - 0.01,
                        "{id} overlaps {} header",
                        group.label
                    );
                    assert!(node.x + node.width <= group.x + group.width + 0.01);
                    assert!(node.y + node.height <= group.y + group.height + 0.01);
                }
            }
        }
        assert_eq!(output.edges.len(), 3);
        assert!(output.edges.iter().all(|edge| edge.points.len() >= 2));
        assert!(output.nodes["D"].y > output.nodes["C"].y);
    }

    #[test]
    fn elk_retains_direction_styles_links_and_explicit_native_selection() {
        for (direction, axis, sign) in [
            ("LR", 0, 1.0),
            ("RL", 0, -1.0),
            ("TD", 1, 1.0),
            ("BT", 1, -1.0),
        ] {
            let parsed = parse_mermaid_strict(&format!("flowchart {direction}\nA[开始] --> B[完成]\nstyle A fill:#FF0000\nlinkStyle 0 stroke:#123456,stroke-width:3px\nclick A \"https://example.com\"")).unwrap();
            let output = elk_layout(
                &parsed.graph,
                &Theme::modern(),
                &LayoutConfig::default(),
                None,
            )
            .unwrap();
            let (a, b) = (&output.nodes["A"], &output.nodes["B"]);
            let delta = if axis == 0 { b.x - a.x } else { b.y - a.y };
            assert!(delta * sign > 0.0, "direction {direction}");
            assert_eq!(a.style.fill.as_deref(), Some("#FF0000"));
            assert_eq!(a.link.as_ref().unwrap().url, "https://example.com");
            assert_eq!(
                output.edges[0].override_style.stroke.as_deref(),
                Some("#123456")
            );
        }
        let parsed = parse_mermaid_strict(
            "%%{init: {\"flowchart\": {\"defaultRenderer\": \"dagre\"}}}%%\nflowchart LR\nA --> B",
        )
        .unwrap();
        assert_eq!(
            layout(
                &parsed.graph,
                &Theme::modern(),
                &LayoutConfig::default(),
                parsed.init_config.as_ref()
            )
            .1,
            Backend::Native
        );
        let parsed = parse_mermaid_strict("sequenceDiagram\nAlice->>Bob: Hello").unwrap();
        assert_eq!(
            layout(
                &parsed.graph,
                &Theme::modern(),
                &LayoutConfig::default(),
                None
            )
            .1,
            Backend::Native
        );
    }

    #[test]
    fn arrows_touch_diamond_and_circle_outlines_with_multiple_ports() {
        let parsed = parse_mermaid_strict(
            "flowchart LR\nA --> D{判断}\nB --> D\nC --> D\nD --> E((结束))\nA --> E\nB --> E",
        )
        .unwrap();
        let output = elk_layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        )
        .unwrap();
        for edge in &output.edges {
            let node = &output.nodes[&edge.to];
            let point = edge.points.last().unwrap();
            let dx = (point.0 - node.x - node.width / 2.0) / (node.width / 2.0);
            let dy = (point.1 - node.y - node.height / 2.0) / (node.height / 2.0);
            let distance = if node.shape == NodeShape::Diamond {
                dx.abs() + dy.abs()
            } else {
                (dx * dx + dy * dy).sqrt()
            };
            assert!(
                (distance - 1.0).abs() < 0.02,
                "{} arrow misses outline: {distance}",
                node.id
            );
        }
    }

    #[test]
    fn group_endpoints_and_wide_headers_do_not_resize_leaf_shapes() {
        let parsed = parse_mermaid_strict("flowchart LR\nsubgraph G[包含多个节点的阶段标题]\nA((开始)) --> B{确认}\nend\nG --> C[结束]").unwrap();
        let output = elk_layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        )
        .unwrap();
        assert_eq!(output.nodes["A"].width, output.nodes["A"].height);
        assert!(output.nodes["G"].hidden);
        assert_eq!(output.edges[1].from, "G");
        let group = &output.subgraphs[0];
        let start = output.edges[1].points[0];
        let boundary = ((start.0 - group.x - group.width / 2.0) / (group.width / 2.0))
            .abs()
            .max(((start.1 - group.y - group.height / 2.0) / (group.height / 2.0)).abs());
        assert!((boundary - 1.0).abs() < 0.01);
    }
}
