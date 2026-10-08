//! A view owns one decoded diagram. GPUI's retain-all asset cache is bypassed.
use crate::diagrams::{Diagram, RasterSize};
use gpui::{Context, Render, RenderImage, Task, Window, div, img, prelude::*};
use std::{sync::Arc, time::Duration};

pub struct DiagramImage {
    diagram: Diagram,
    requested: Option<RasterSize>,
    image: Option<Arc<RenderImage>>,
    task: Option<Task<()>>,
    error: Option<String>,
}

impl DiagramImage {
    pub fn new(diagram: Diagram, cx: &mut Context<Self>) -> Self {
        cx.on_release(|this, cx| {
            if let Some(image) = this.image.take() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        Self {
            diagram,
            requested: None,
            image: None,
            task: None,
            error: None,
        }
    }

    pub(crate) fn is_for(&self, diagram: &Diagram) -> bool {
        Arc::ptr_eq(&self.diagram.image, &diagram.image)
    }

    pub fn set_display_size(&mut self, width: f32, height: f32, dpi: f32, cx: &mut Context<Self>) {
        let requested = RasterSize::for_display(width, height, dpi);
        if self.requested == Some(requested) {
            return;
        }
        self.requested = Some(requested);
        self.error = None;
        let diagram = self.diagram.clone();
        let renderer = cx.svg_renderer();
        let executor = cx.background_executor().clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            // Coalesce window resizing and repeated zoom gestures before allocating pixels.
            executor.timer(Duration::from_millis(100)).await;
            let result = executor
                .spawn(async move {
                    diagram
                        .image_at_size(requested)
                        .to_image_data(renderer)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.requested != Some(requested) {
                    return;
                }
                match result {
                    Ok(image) => {
                        if let Some(old) = this.image.replace(image) {
                            cx.drop_image(old, None);
                        }
                    }
                    Err(error) => this.error = Some(error),
                }
                this.task = None;
                cx.notify();
            });
        }));
    }
}

impl Render for DiagramImage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .when_some(self.image.clone(), |el, image| {
                el.child(img(image).size_full())
            })
            .when(self.image.is_none(), |el| {
                el.child(if self.error.is_some() {
                    "流程图显示失败"
                } else {
                    "正在生成预览…"
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::{self, DiagramKey};
    use gpui::TestAppContext;

    #[gpui::test]
    fn resize_reuses_equal_targets_and_releases_obsolete_pixels(cx: &mut TestAppContext) {
        let diagram = diagrams::render(&DiagramKey {
            source: "flowchart LR\nA --> B".into(),
            dark: false,
        })
        .unwrap();
        let view = cx.new(|cx| DiagramImage::new(diagram, cx));
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(101));
        cx.run_until_parked();
        let first = cx.read(|cx| view.read(cx).image.clone().unwrap());
        let weak = Arc::downgrade(&first);
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        assert!(cx.read(|cx| Arc::ptr_eq(&first, view.read(cx).image.as_ref().unwrap())));
        drop(first);
        view.update(cx, |view, cx| view.set_display_size(200.0, 70.0, 2.0, cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(101));
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        let weak = cx.read(|cx| Arc::downgrade(view.read(cx).image.as_ref().unwrap()));
        drop(view);
        // GPUI releases dropped entities at the next application update boundary.
        cx.update(|_| ());
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
    }
}
