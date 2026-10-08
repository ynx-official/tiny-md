//! A view owns one decoded diagram. GPUI's retain-all asset cache is bypassed.
use crate::diagrams::{Diagram, DiagramRegion, RasterSize};
use gpui::{App, Context, Render, RenderImage, Task, Window, div, img, prelude::*, px};
use std::{sync::Arc, time::Duration};

pub struct DiagramImage {
    diagram: Diagram,
    requested: Option<RasterRequest>,
    image: Option<Arc<RenderImage>>,
    task: Option<Task<()>>,
    error: Option<String>,
    high_definition: bool,
    unloaded: bool,
    decoded_region: Option<DiagramRegion>,
    presentation: Option<((f32, f32), f32)>,
}

#[derive(Clone, Copy, PartialEq)]
struct RasterRequest {
    size: RasterSize,
    region: Option<DiagramRegion>,
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
            high_definition: false,
            unloaded: false,
            decoded_region: None,
            presentation: None,
        }
    }

    /// The independent viewer re-rasterizes the vector source, never the inline bitmap.
    pub fn high_definition(diagram: Diagram, cx: &mut Context<Self>) -> Self {
        Self {
            high_definition: true,
            ..Self::new(diagram, cx)
        }
    }

    pub fn decoded_size(&self) -> Option<RasterSize> {
        self.image.as_ref().map(|image| {
            let size = image.size(0);
            RasterSize {
                width: size.width.0 as u32,
                height: size.height.0 as u32,
            }
        })
    }

    /// Stop pending decoding and evict this window's textures before it is destroyed.
    pub fn unload(&mut self, window: &mut Window, cx: &mut App) {
        self.unloaded = true;
        self.requested = None;
        self.task = None;
        self.error = None;
        self.decoded_region = None;
        self.presentation = None;
        if let Some(image) = self.image.take() {
            cx.drop_image(image, Some(window));
        }
    }

    pub(crate) fn is_for(&self, diagram: &Diagram) -> bool {
        Arc::ptr_eq(&self.diagram.image, &diagram.image)
    }

    pub fn set_display_size(&mut self, width: f32, height: f32, dpi: f32, cx: &mut Context<Self>) {
        if self.unloaded {
            return;
        }
        let requested = if self.high_definition {
            // Fit-to-window must not turn a large vector diagram into a tiny bitmap.
            // Keep at least two pixels per logical SVG unit, then follow larger zoom.
            let scale =
                ((width / self.diagram.width).max(height / self.diagram.height) * dpi).max(2.0);
            RasterSize::for_display(self.diagram.width * scale, self.diagram.height * scale, 1.0)
        } else {
            RasterSize::for_display(width, height, dpi)
        };
        self.presentation = None;
        self.request(
            RasterRequest {
                size: requested,
                region: None,
            },
            cx,
        );
    }

    /// Render the visible vector region at twice the display pixel density.
    pub fn set_viewport(
        &mut self,
        size: (f32, f32),
        origin: (f32, f32),
        scale: f32,
        dpi: f32,
        cx: &mut Context<Self>,
    ) {
        if self.unloaded {
            return;
        }
        let margin = 128.0;
        let scale = scale.max(0.0001);
        let region = DiagramRegion {
            x: (-origin.0 - margin) / scale,
            y: (-origin.1 - margin) / scale,
            width: (size.0.max(1.0) + margin * 2.0) / scale,
            height: (size.1.max(1.0) + margin * 2.0) / scale,
        };
        let presentation = Some((origin, scale));
        let moved = self.presentation != presentation;
        self.presentation = presentation;
        let density = if self.high_definition { 2.0 } else { 1.0 };
        let pixels =
            RasterSize::for_display(size.0 + margin * 2.0, size.1 + margin * 2.0, dpi * density);
        self.request(
            RasterRequest {
                size: pixels,
                region: Some(region),
            },
            cx,
        );
        // Translate the previous region immediately while the next one decodes.
        if moved {
            cx.notify();
        }
    }

    fn request(&mut self, requested: RasterRequest, cx: &mut Context<Self>) {
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
                    let source = match requested.region {
                        Some(region) => diagram.image_in_region(requested.size, region),
                        None => diagram.image_at_size(requested.size),
                    };
                    source
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
                        this.decoded_region = requested.region;
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
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .when_some(self.image.clone(), |el, image| {
                if let Some((region, (origin, scale))) = self.decoded_region.zip(self.presentation)
                {
                    el.child(
                        img(image)
                            .absolute()
                            .left(px(origin.0 + region.x * scale))
                            .top(px(origin.1 + region.y * scale))
                            .w(px(region.width * scale))
                            .h(px(region.height * scale)),
                    )
                } else {
                    el.child(img(image).size_full())
                }
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
    fn high_definition_crops_the_vector_and_rerenders_after_panning(cx: &mut TestAppContext) {
        let diagram = Diagram {
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="12000" height="2000" viewBox="0 0 12000 2000"><rect width="6000" height="2000" fill="#ff0000"/><rect x="6000" width="6000" height="2000" fill="#0000ff"/></svg>"##.to_vec())),
            width: 12000.0, height: 2000.0,
        };
        let image = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        image.update(cx, |image, cx| {
            image.set_viewport((600.0, 400.0), (0.0, 0.0), 1.0, 1.0, cx)
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(101));
        cx.run_until_parked();
        let first = cx.read(|cx| image.read(cx).image.clone().unwrap());
        let size = first.size(0);
        assert_eq!(size.width.0, 1712);
        assert_eq!(size.height.0, 1312);
        assert!(first.as_bytes(0).unwrap().len() < 5 * 1024 * 1024 * 2);
        let center = ((size.height.0 / 2 * size.width.0 + size.width.0 / 2) * 4) as usize;
        let red = first.as_bytes(0).unwrap()[center..center + 4].to_vec();
        let weak = Arc::downgrade(&first);
        drop(first);
        image.update(cx, |image, cx| {
            image.set_viewport((600.0, 400.0), (-8000.0, 0.0), 1.0, 1.0, cx)
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(101));
        cx.run_until_parked();
        cx.read(|cx| {
            let second = image.read(cx).image.as_ref().unwrap();
            assert_ne!(red, second.as_bytes(0).unwrap()[center..center + 4]);
        });
        assert!(
            weak.upgrade().is_none(),
            "panning must release the previous region's pixels"
        );
    }

    #[gpui::test]
    fn closing_cancels_pending_high_definition_work_and_prevents_reallocation(
        cx: &mut TestAppContext,
    ) {
        let diagram = diagrams::render(&DiagramKey {
            source: "flowchart LR\nA --> B".into(),
            dark: false,
        })
        .unwrap();
        let image = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        let window = cx.add_empty_window();
        window.update(|_, cx| {
            image.update(cx, |image, cx| {
                image.set_viewport((600.0, 400.0), (0.0, 0.0), 1.0, 1.0, cx)
            })
        });
        window.run_until_parked();
        window.update(|window, cx| image.update(cx, |image, cx| image.unload(window, cx)));
        window
            .background_executor
            .advance_clock(Duration::from_millis(101));
        window.run_until_parked();
        window.update(|_, cx| {
            image.update(cx, |image, cx| {
                image.set_viewport((600.0, 400.0), (0.0, 0.0), 1.0, 1.0, cx)
            })
        });
        window.run_until_parked();
        window.read(|cx| {
            let image = image.read(cx);
            assert!(image.decoded_size().is_none());
            assert!(image.task.is_none());
            assert!(image.requested.is_none());
        });
    }

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
