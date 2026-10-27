//! Image state belongs to the document window, while loading stays off the UI thread.
use super::*;
use crate::markdown_images::{self, ImageLink};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

pub(super) enum State {
    Loading { _task: Task<()>, generation: u64 },
    Ready(Arc<gpui::RenderImage>),
    Error(String),
}

#[derive(Default)]
pub(super) struct Images {
    pub document: Option<PathBuf>,
    pub entries: HashMap<String, State>,
    generation: u64,
    heights: Vec<Vec<f32>>,
    pub flows: HashMap<(u64, u32), super::image_flow::Layout>,
}

impl Images {
    pub fn clear(&mut self, cx: &mut App) {
        for (_, state) in self.entries.drain() {
            if let State::Ready(image) = state {
                release(image, cx);
            }
        }
        self.heights.clear();
        self.flows.clear();
        self.generation = self.generation.wrapping_add(1);
    }
}

fn release(image: Arc<gpui::RenderImage>, cx: &mut App) {
    // Defer until the current window returns to App.windows, so its GPU texture
    // is evicted too (set_text and render normally run with that window borrowed).
    cx.defer(move |cx| cx.drop_image(image, None));
}

impl MarkdownEditor {
    /// Set the Markdown file location, including after first save / Save As.
    /// Relative image targets are never resolved against the process directory.
    pub fn set_document_path(&mut self, path: Option<&Path>, cx: &mut Context<Self>) {
        if self.images.document.as_deref() != path {
            self.images.clear(cx);
            self.images.document = path.map(Path::to_owned);
            self.diagram_geometry_dirty = true;
            cx.notify();
        }
    }

    /// Current base file for resolving local image links.
    pub fn document_path(&self) -> Option<&Path> {
        self.images.document.as_deref()
    }

    pub(super) fn sync_images(&mut self, rows: &[Vec<ImageLink>], cx: &mut Context<Self>) {
        let targets: HashSet<_> = rows
            .iter()
            .flatten()
            .map(|image| image.target.as_str())
            .collect();
        self.images.entries.retain(|target, state| {
            if targets.contains(target.as_str()) {
                return true;
            }
            if let State::Ready(image) = state {
                release(image.clone(), cx);
            }
            false
        });
        if self.source_mode {
            return;
        }
        // Enqueue in document order. Repeated targets share one download and bitmap.
        for image in rows.iter().flatten() {
            if self.images.entries.contains_key(&image.target) {
                continue;
            }
            let target =
                match markdown_images::resolve(&image.target, self.images.document.as_deref()) {
                    Ok(target) => target,
                    Err(error) => {
                        self.images
                            .entries
                            .insert(image.target.clone(), State::Error(error));
                        continue;
                    }
                };
            static CAPACITY: LazyLock<Arc<async_lock::Semaphore>> =
                LazyLock::new(|| Arc::new(async_lock::Semaphore::new(4)));
            let capacity = CAPACITY.clone();
            let source = image.target.clone();
            self.images.generation = self.images.generation.wrapping_add(1);
            let generation = self.images.generation;
            let executor = cx.background_executor().clone();
            let task = cx.spawn(async move |this, cx| {
                let permit = capacity.acquire_arc().await;
                let result = executor.spawn(async move {
                    let _permit = permit;
                    markdown_images::load(&target)
                }).await;
                let _ = this.update(cx, |this, cx| {
                    if !matches!(this.images.entries.get(&source), Some(State::Loading { generation: active, .. }) if *active == generation) { return; }
                    this.images.entries.insert(source, match result {
                        Ok(image) => State::Ready(image), Err(error) => State::Error(error),
                    });
                    this.diagram_geometry_dirty = true;
                    cx.notify();
                });
            });
            self.images.entries.insert(
                image.target.clone(),
                State::Loading {
                    _task: task,
                    generation,
                },
            );
        }
    }

    pub(super) fn image_heights(
        &mut self,
        rows: &[Vec<ImageLink>],
        width: f32,
        dragging: bool,
    ) -> Vec<Vec<f32>> {
        if self.source_mode {
            return vec![Vec::new(); rows.len()];
        }
        let heights = rows
            .iter()
            .enumerate()
            .map(|(line, images)| {
                images
                    .iter()
                    .enumerate()
                    .map(|(index, link)| {
                        if dragging
                            && let Some(height) =
                                self.images.heights.get(line).and_then(|row| row.get(index))
                        {
                            return *height;
                        }
                        let height = match self.images.entries.get(&link.target) {
                            Some(State::Ready(image)) => {
                                markdown_images::display_size_for(image, width, link).1
                            }
                            _ => 96.0,
                        };
                        height + markdown_images::GAP * 2.0
                    })
                    .collect()
            })
            .collect::<Vec<_>>();
        self.images.heights = heights.clone();
        heights
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn image_elements(
        &self,
        line: usize,
        links: &[ImageLink],
        heights: &[f32],
        width: f32,
        row_height: f32,
        border: Hsla,
        muted: Hsla,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut group = div()
            .absolute()
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .top(px(row_height - heights.iter().sum::<f32>()));
        for (index, (link, height)) in links.iter().zip(heights).enumerate() {
            let mut preview = div()
                .id((
                    SharedString::from(format!("markdown-image-{line}-{}", link.source_offset)),
                    index,
                ))
                .debug_selector(|| "markdown-image".into())
                .w_full()
                .h(px(*height))
                .flex_shrink_0()
                .py(px(markdown_images::GAP))
                .overflow_hidden();
            match self.images.entries.get(&link.target) {
                Some(State::Ready(image)) => {
                    let (w, h) = markdown_images::display_size_for(image, width, link);
                    preview = preview.flex();
                    preview = match link.align {
                        1 => preview.justify_center(),
                        2 => preview.justify_end(),
                        _ => preview,
                    };
                    preview = preview.child(
                        gpui::img(image.clone())
                            .id((
                                SharedString::from(format!(
                                    "markdown-image-pixels-{line}-{}",
                                    link.source_offset
                                )),
                                index,
                            ))
                            .debug_selector(|| "markdown-image-pixels".into())
                            .w(px(w))
                            .h(px(h)),
                    );
                }
                state => {
                    let error = match state {
                        Some(State::Error(error)) => Some(error),
                        _ => None,
                    };
                    let label = if link.alt.is_empty() {
                        "图片"
                    } else {
                        &link.alt
                    };
                    let mut message = div()
                        .size_full()
                        .border_1()
                        .border_color(border)
                        .rounded(px(6.0))
                        .px(px(12.0))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(4.0))
                        .text_size(px(13.0))
                        .text_color(muted)
                        .overflow_hidden()
                        .child(div().text_ellipsis().child(label.to_owned()))
                        .child(
                            div()
                                .text_ellipsis()
                                .child(error.cloned().unwrap_or_else(|| "正在加载图片……".into())),
                        );
                    if error.is_some() {
                        let target = link.target.clone();
                        message = message.child(
                            div()
                                .id((
                                    SharedString::from(format!(
                                        "retry-image-{line}-{}",
                                        link.source_offset
                                    )),
                                    index,
                                ))
                                .debug_selector(|| "markdown-image-retry".into())
                                .cursor_pointer()
                                .child("重试")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.images.entries.remove(&target);
                                        cx.notify();
                                    }),
                                ),
                        );
                    }
                    preview = preview.child(message);
                }
            }
            if !self.read_only || link.href.is_some() {
                let href = link.href.clone();
                preview = preview.cursor_pointer().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.finish_composition(cx);
                        if (this.read_only || event.modifiers.cmd())
                            && let Some(href) = &href
                        {
                            cx.emit(MarkdownEditorEvent::LinkClick(href.clone()));
                            return;
                        }
                        if this.read_only {
                            return;
                        }
                        this.model.move_to(line, 0, false);
                        this.caret.reset();
                        window.focus(&this.focus);
                        cx.notify();
                    }),
                );
            }
            group = group.child(preview);
        }
        group
    }
}
