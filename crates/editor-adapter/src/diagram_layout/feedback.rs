use super::clip_endpoint;
use mermaid_rs_renderer::layout::{Layout, TextBlock};

type Point = (f32, f32);
type Bounds = [Point; 2];

struct FeedbackRoute {
    edge: usize,
    points: [Point; 4],
    anchor: Option<Point>,
}

pub(super) fn straighten_feedback_routes(layout: &mut Layout, entry_id: &str) {
    let entry = &layout.nodes[entry_id];
    let right = layout
        .nodes
        .values()
        .map(|node| node.x + node.width)
        .fold(0.0, f32::max);
    let mut lanes = Vec::new();
    for (index, edge) in layout.edges.iter().enumerate() {
        if edge.to != entry_id || edge.from == entry_id {
            continue;
        }
        if edge.start_label.is_some() || edge.end_label.is_some() {
            return;
        }
        let Some(lane) = edge
            .points
            .windows(2)
            .filter_map(|segment| {
                let [start, end] = [segment[0], segment[1]];
                (start.0 > right
                    && (start.0 - end.0).abs() < 0.1
                    && (start.1 - end.1).abs() > entry.height)
                    .then_some(start.0)
            })
            .max_by(f32::total_cmp)
        else {
            return;
        };
        lanes.push((index, lane));
    }
    let port_spacing = (entry.height / (lanes.len() + 1) as f32).min(14.0);
    if lanes.is_empty() || port_spacing < 8.0 {
        return;
    }
    lanes.sort_by(|a, b| a.1.total_cmp(&b.1));
    if lanes.windows(2).any(|pair| pair[1].1 - pair[0].1 < 8.0) {
        return;
    }
    let mut routes = Vec::with_capacity(lanes.len());
    for (order, &(edge_index, lane)) in lanes.iter().enumerate() {
        let edge = &layout.edges[edge_index];
        let source = &layout.nodes[&edge.from];
        let start = clip_endpoint(
            source,
            (source.x + source.width, source.y + source.height * 0.66),
        );
        // Outer returns approach the higher ports, clear of the inner returns.
        let port_y = entry.y
            + entry.height / 2.0
            + ((lanes.len() - 1) as f32 / 2.0 - order as f32) * port_spacing;
        let end = clip_endpoint(entry, (entry.x + entry.width, port_y));
        let rise_to_entry = ((lane - end.0) * 0.05).clamp(10.0, 28.0);
        let drop_from_source = ((lane - start.0) * 0.13).clamp(12.0, 50.0);
        if start.1 <= end.1 + rise_to_entry {
            return;
        }
        let mut anchor = None;
        if let Some(label) = &edge.label {
            let offset = source.x + source.width - start.0 + label.width / 2.0 + 12.0;
            if start.0 + offset + label.width / 2.0 + 8.0 >= lane {
                return;
            }
            anchor = Some((
                start.0 + offset,
                start.1 + drop_from_source * offset / (lane - start.0),
            ));
        }
        routes.push(FeedbackRoute {
            edge: edge_index,
            points: [
                start,
                (lane, start.1 + drop_from_source),
                (lane, end.1 + rise_to_entry),
                end,
            ],
            anchor,
        });
    }
    // Validate the entire set before changing the ELK routes. A blocked approach
    // can make a mixture of original and shortened returns cross at the entry.
    if routes
        .iter()
        .all(|route| feedback_route_is_clear(layout, &routes, route))
    {
        for route in routes {
            let edge = &mut layout.edges[route.edge];
            edge.points = route.points.into();
            edge.label_anchor = route.anchor;
        }
    }
}

fn feedback_route_is_clear(
    layout: &Layout,
    routes: &[FeedbackRoute],
    route: &FeedbackRoute,
) -> bool {
    let edge = &layout.edges[route.edge];
    let badge = label_bounds(edge.label.as_ref(), route.anchor);
    for node in layout.nodes.values() {
        let bounds = [
            (node.x, node.y),
            (node.x + node.width, node.y + node.height),
        ];
        if badge.is_some_and(|label| bounds_overlap(label, bounds))
            || (node.id != edge.from
                && node.id != edge.to
                && route
                    .points
                    .windows(2)
                    .any(|segment| crosses_bounds(segment, bounds)))
        {
            return false;
        }
    }
    for (index, other) in layout
        .edges
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != route.edge)
    {
        let replacement = routes.iter().find(|candidate| candidate.edge == index);
        let points = replacement.map_or(other.points.as_slice(), |candidate| {
            candidate.points.as_slice()
        });
        let anchor = replacement.map_or(other.label_anchor, |candidate| candidate.anchor);
        let other_badges = [
            label_bounds(other.label.as_ref(), anchor),
            label_bounds(other.start_label.as_ref(), other.start_label_anchor),
            label_bounds(other.end_label.as_ref(), other.end_label_anchor),
        ];
        if other_badges.into_iter().flatten().any(|other_badge| {
            badge.is_some_and(|label| bounds_overlap(label, other_badge))
                || route
                    .points
                    .windows(2)
                    .any(|segment| crosses_bounds(segment, other_badge))
        }) || points.windows(2).any(|segment| {
            badge.is_some_and(|label| crosses_bounds(segment, label))
                || route
                    .points
                    .windows(2)
                    .any(|new_segment| segments_cross(new_segment, segment))
        }) {
            return false;
        }
    }
    true
}

fn label_bounds(label: Option<&TextBlock>, anchor: Option<Point>) -> Option<Bounds> {
    let (label, (x, y)) = (label?, anchor?);
    let half_width = label.width / 2.0 + 5.0;
    let half_height = label.height / 2.0 + 3.0;
    Some([
        (x - half_width, y - half_height),
        (x + half_width, y + half_height),
    ])
}

fn bounds_overlap(a: Bounds, b: Bounds) -> bool {
    a[0].0 < b[1].0 && b[0].0 < a[1].0 && a[0].1 < b[1].1 && b[0].1 < a[1].1
}

pub(super) fn crosses_bounds(segment: &[Point], [(x_min, y_min), (x_max, y_max)]: Bounds) -> bool {
    let [(x1, y1), (x2, y2)] = [segment[0], segment[1]];
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    for (start, end, min, max) in [(x1, x2, x_min, x_max), (y1, y2, y_min, y_max)] {
        let delta = end - start;
        if delta.abs() < 0.001 {
            if start <= min || start >= max {
                return false;
            }
        } else {
            let a = (min - start) / delta;
            let b = (max - start) / delta;
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
    }
    enter < leave
}

fn segments_cross(a: &[Point], b: &[Point]) -> bool {
    let difference = |p: Point, q: Point| (p.0 - q.0, p.1 - q.1);
    let cross = |p: Point, q: Point| p.0 * q.1 - p.1 * q.0;
    let (r, s) = (difference(a[1], a[0]), difference(b[1], b[0]));
    let area = cross(r, s);
    if area.abs() < 0.001 {
        return false;
    }
    let offset = difference(b[0], a[0]);
    let (t, u) = (cross(offset, s) / area, cross(offset, r) / area);
    let on_segment = 0.0..=1.0;
    let interior = 0.001..0.999;
    on_segment.contains(&t)
        && on_segment.contains(&u)
        && (interior.contains(&t) || interior.contains(&u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::{LayoutConfig, Theme};

    #[test]
    fn blocked_shortcut_preserves_the_existing_return_route() {
        let source = include_str!("../../../../fixtures/diagrams/market-violation-workflow.mmd");
        let parsed = crate::mermaid_parse::parse(source).unwrap();
        let (mut layout, _) = super::super::layout(
            &parsed.graph,
            &Theme::modern(),
            &LayoutConfig::default(),
            None,
        );
        let index = layout
            .edges
            .iter()
            .position(|edge| edge.from == "market" && edge.to == "entry")
            .unwrap();
        let [start, bend, _, _] = layout.edges[index].points[..] else {
            panic!("expected a four-point return route");
        };
        let midpoint = ((start.0 + bend.0) / 2.0, (start.1 + bend.1) / 2.0);
        let mut obstacle = layout.nodes["renew"].clone();
        obstacle.id = "obstacle".into();
        (obstacle.x, obstacle.y) = (midpoint.0 - 6.0, midpoint.1 - 6.0);
        (obstacle.width, obstacle.height) = (12.0, 12.0);
        layout.nodes.insert(obstacle.id.clone(), obstacle);
        layout.edges[index]
            .points
            .insert(1, (midpoint.0, midpoint.1 - 30.0));
        let detour = layout.edges[index].points.clone();

        straighten_feedback_routes(&mut layout, "entry");

        assert_eq!(layout.edges[index].points, detour);
    }
}
