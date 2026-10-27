//! Image-only HTML containers use inline flow; source lines remain editing units.
use super::*;
use crate::{
    html,
    markdown_images::{self, ImageLink},
};

#[derive(Clone)]
pub(super) struct Item {
    link: ImageLink,
    line: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone)]
pub(super) struct Layout {
    items: Vec<Item>,
    pub height: f32,
}

fn finish_row(items: &mut [Item], used: f32, available: f32, align: u8) {
    let shift = (available - used).max(0.0)
        * match align {
            1 => 0.5,
            2 => 1.0,
            _ => 0.0,
        };
    for item in items {
        item.x += shift;
    }
}

impl MarkdownEditor {
    pub(super) fn image_flows(
        &mut self,
        html: &html::Document,
        images: &[Vec<ImageLink>],
        visible: &[bool],
        width: f32,
        dragging: bool,
    ) -> Vec<Option<Layout>> {
        let mut layouts = vec![None; images.len()];
        self.images.flows.retain(|(signature, bits), _| {
            *bits == width.to_bits() && html.image_groups.iter().any(|g| g.signature == *signature)
        });
        for (group, visible) in html.image_groups.iter().zip(visible) {
            if !visible {
                continue;
            }
            let key = (group.signature, width.to_bits());
            if dragging && let Some(layout) = self.images.flows.get(&key) {
                layouts[html.line(group.range.start)] = Some(layout.clone());
                continue;
            }
            let mut items = Vec::new();
            let (mut x, mut y, mut row_height) = (0.0_f32, 0.0_f32, 0.0_f32);
            let (mut row_start, mut previous) = (0, group.range.start);
            const GAP: f32 = 8.0;
            for link in images
                .iter()
                .flatten()
                .filter(|link| group.range.contains(&link.source_offset))
            {
                let (w, h) = match self.images.entries.get(&link.target) {
                    Some(editor_images::State::Ready(image)) => {
                        markdown_images::display_size_for(image, width, link)
                    }
                    Some(editor_images::State::Error(_)) => (160.0_f32.min(width), 96.0),
                    _ => (
                        link.width.unwrap_or(160.0).min(width).max(1.0),
                        link.height.unwrap_or(48.0).clamp(20.0, 96.0),
                    ),
                };
                let forced_break = group
                    .breaks
                    .iter()
                    .any(|&at| at > previous && at < link.source_offset);
                if !items[row_start..].is_empty() && (forced_break || x + w > width) {
                    finish_row(&mut items[row_start..], x - GAP, width, group.align);
                    y += row_height;
                    row_height = 0.0;
                    x = 0.0;
                    row_start = items.len();
                }
                let height = h + markdown_images::GAP * 2.0;
                let mut inline = link.clone();
                inline.align = 0;
                items.push(Item {
                    link: inline,
                    line: html.line(link.source_offset),
                    x,
                    y,
                    width: w,
                    height,
                });
                x += w + GAP;
                row_height = row_height.max(height);
                previous = link.source_offset;
            }
            finish_row(
                &mut items[row_start..],
                (x - GAP).max(0.0),
                width,
                group.align,
            );
            let layout = Layout {
                items,
                height: y + row_height,
            };
            self.images.flows.insert(key, layout.clone());
            layouts[html.line(group.range.start)] = Some(layout);
        }
        layouts
    }

    pub(super) fn image_flow_element(
        &self,
        layout: &Layout,
        row_height: f32,
        border: Hsla,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut group = div()
            .absolute()
            .left_0()
            .top(px(row_height - layout.height))
            .w_full()
            .h(px(layout.height))
            .debug_selector(|| "html-image-flow".into());
        for (index, item) in layout.items.iter().enumerate() {
            group = group.child(
                div()
                    .absolute()
                    .left(px(item.x))
                    .top(px(item.y))
                    .w(px(item.width))
                    .h(px(item.height))
                    .debug_selector(move || format!("html-inline-image-{index}"))
                    .child(self.image_elements(
                        item.line,
                        std::slice::from_ref(&item.link),
                        &[item.height],
                        item.width,
                        item.height,
                        border,
                        muted,
                        cx,
                    )),
            );
        }
        group
    }
}
