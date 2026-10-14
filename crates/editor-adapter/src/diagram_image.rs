//! Previews share active pixels; the last owner releases their CPU and GPU data.
use crate::diagram_cache::{self, RasterKey};
use crate::diagrams::{Diagram, RasterSize};
use gpui::{App, Context, Render, RenderImage, Task, Window, div, img, prelude::*, px};
use std::{sync::Arc, time::Duration};

pub struct DiagramImage {
    diagram: Diagram,
    requested: Option<RasterSize>,
    image: Option<Arc<RenderImage>>,
    task: Option<Task<()>>,
    error: Option<String>,
    high_definition: bool,
    unloaded: bool,
    presentation: Option<((f32, f32), f32)>,
}

impl DiagramImage {
    pub fn new(diagram: Diagram, cx: &mut Context<Self>) -> Self {
        cx.on_release(|this, cx| {
            if let Some(image) = this.image.take() {
                diagram_cache::release(cx, image, None);
            }
            cx.defer(diagram_cache::prune_idle);
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
            presentation: None,
        }
    }

    /// Decode one fixed high-resolution full diagram when the viewer opens.
    pub fn high_definition(diagram: Diagram, cx: &mut Context<Self>) -> Self {
        let requested = RasterSize::for_viewer(diagram.width, diagram.height);
        let mut this = Self {
            high_definition: true,
            ..Self::new(diagram, cx)
        };
        this.request(requested, cx);
        this
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

    #[cfg(test)]
    pub(crate) fn decoded_id(&self) -> Option<gpui::ImageId> {
        self.image.as_ref().map(|image| image.id)
    }

    #[cfg(test)]
    pub(crate) fn decoded_reference(&self) -> Option<std::sync::Weak<RenderImage>> {
        self.image.as_ref().map(Arc::downgrade)
    }

    /// Stop pending decoding and evict this window's texture before it is destroyed.
    pub fn unload(&mut self, window: &mut Window, cx: &mut App) {
        self.unloaded = true;
        self.requested = None;
        self.task = None;
        self.error = None;
        self.presentation = None;
        if let Some(image) = self.image.take() {
            diagram_cache::release(cx, image, Some(window));
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
            RasterSize::for_viewer(self.diagram.width, self.diagram.height)
        } else {
            RasterSize::for_display(width, height, dpi)
        };
        self.presentation = None;
        self.request(requested, cx);
    }

    /// Only change presentation. Window size, DPI and zoom never request new pixels.
    pub fn set_viewport(
        &mut self,
        _size: (f32, f32),
        origin: (f32, f32),
        scale: f32,
        _dpi: f32,
        cx: &mut Context<Self>,
    ) {
        if self.unloaded {
            return;
        }
        let presentation = Some((origin, scale.max(0.0001)));
        if self.presentation != presentation {
            self.presentation = presentation;
            cx.notify();
        }
    }

    fn request(&mut self, requested: RasterSize, cx: &mut Context<Self>) {
        if self.requested == Some(requested) {
            return;
        }
        self.requested = Some(requested);
        self.error = None;
        let cache_key = RasterKey::new(&self.diagram, requested);
        if !self.high_definition
            && let Some(image) = diagram_cache::pixels(cx, &cache_key)
        {
            self.task = None;
            self.replace_image(image, cx);
            cx.notify();
            return;
        }
        let diagram = self.diagram.clone();
        let executor = cx.background_executor().clone();
        let high_definition = self.high_definition;
        let capacity = crate::diagram_work::raster(cx);
        let ticket = crate::diagram_work::reserve_raster(cx);
        let request = (!high_definition).then(|| diagram_cache::request_raster(cx, &cache_key));
        self.task = Some(cx.spawn(async move |this, cx| {
            // Inline previews coalesce resizing; the independent viewer skips debounce.
            if !high_definition {
                executor.timer(Duration::from_millis(100)).await;
            }
            let ticket = ticket.wait().await;
            let request_permit = if let Some(request) = request {
                Some(request.acquire().await)
            } else {
                None
            };
            let Ok(cached) = this.update(cx, |_, cx| {
                (!high_definition)
                    .then(|| diagram_cache::pixels(cx, &cache_key))
                    .flatten()
            }) else {
                return;
            };
            // Preview, viewer and layout jobs share the five CPU slots, bounding
            // temporary pixel allocations without blocking the UI threads.
            let (result, ticket, request_permit) = if let Some(image) = cached {
                (Ok(image), ticket, request_permit)
            } else {
                let permit = capacity.acquire_arc().await;
                executor
                    .spawn(async move {
                        let _permit = permit;
                        let result = crate::diagram_raster::decode(&diagram, requested);
                        (result, ticket, request_permit)
                    })
                    .await
            };
            let accepted = this
                .update(cx, |this, cx| {
                    if this.unloaded || this.requested != Some(requested) {
                        return false;
                    }
                    if !high_definition && let Ok(image) = &result {
                        diagram_cache::store_pixels(cx, cache_key, image.clone());
                    }
                    true
                })
                .unwrap_or(false);
            drop(request_permit);
            if !accepted {
                return;
            }
            ticket.publish().await;
            let _ = this.update(cx, |this, cx| {
                if this.unloaded || this.requested != Some(requested) {
                    return;
                }
                match result {
                    Ok(image) => {
                        this.replace_image(image, cx);
                    }
                    Err(error) => this.error = Some(error),
                }
                this.task = None;
                cx.notify();
            });
        }));
    }

    fn replace_image(&mut self, image: Arc<RenderImage>, cx: &mut Context<Self>) {
        if self
            .image
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &image))
        {
            return;
        }
        diagram_cache::pin(cx, &image);
        if let Some(old) = self.image.replace(image) {
            diagram_cache::release(cx, old, None);
        }
    }
}

impl Render for DiagramImage {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // The independent viewer also embeds this view, without Guise globals.
        let text = window.text_style().color;
        let surface = gpui::Hsla {
            a: text.a * 0.15,
            ..text
        };
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .when_some(self.image.clone(), |el, image| {
                if let Some((origin, scale)) = self.presentation {
                    el.child(
                        img(image)
                            .absolute()
                            .left(px(origin.0))
                            .top(px(origin.1))
                            .w(px(self.diagram.width * scale))
                            .h(px(self.diagram.height * scale)),
                    )
                } else {
                    el.child(img(image).size_full())
                }
            })
            .when(self.image.is_none(), |el| {
                if self.error.is_some() {
                    el.child("流程图显示失败")
                } else {
                    el.child(crate::diagrams::skeleton(surface, text))
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::{self, DiagramKey};
    use gpui::TestAppContext;

    fn small_diagram() -> Diagram {
        diagrams::render(&DiagramKey {
            source: "flowchart LR\nA[开始] --> B[结束]".into(),
            dark: false,
        })
        .unwrap()
    }

    fn finish_decoding(cx: &mut TestAppContext) {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(101));
        cx.run_until_parked();
    }

    #[gpui::test]
    fn idle_preview_pixels_release_without_clearing_any_cache(cx: &mut TestAppContext) {
        let view = cx.new(|cx| DiagramImage::new(small_diagram(), cx));
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        finish_decoding(cx);
        let pixels = cx.read(|cx| Arc::downgrade(view.read(cx).image.as_ref().unwrap()));
        drop(view);
        cx.update(|_| ());
        cx.run_until_parked();
        assert!(
            pixels.upgrade().is_none(),
            "closing the last preview must release CPU pixels without explicit cache eviction"
        );
    }

    #[gpui::test]
    fn idle_previous_resolution_releases_after_replacement(cx: &mut TestAppContext) {
        let view = cx.new(|cx| DiagramImage::new(small_diagram(), cx));
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        finish_decoding(cx);
        let previous = cx.read(|cx| Arc::downgrade(view.read(cx).image.as_ref().unwrap()));
        view.update(cx, |view, cx| view.set_display_size(400.0, 120.0, 2.0, cx));
        finish_decoding(cx);
        assert!(cx.read(|cx| view.read(cx).decoded_size().is_some()));
        assert!(
            previous.upgrade().is_none(),
            "a replaced resolution must not remain in a session cache"
        );
    }

    #[gpui::test(iterations = 5)]
    fn diagram_cache_parallel_previews_share_the_same_pixels(cx: &mut TestAppContext) {
        let diagram = small_diagram();
        let views = (0..5)
            .map(|_| {
                let view = cx.new(|cx| DiagramImage::new(diagram.clone(), cx));
                view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
                view
            })
            .collect::<Vec<_>>();
        finish_decoding(cx);
        cx.read(|cx| {
            let first = views[0].read(cx).image.as_ref().unwrap();
            assert!(
                views.iter().all(|view| {
                    view.read(cx)
                        .image
                        .as_ref()
                        .is_some_and(|image| Arc::ptr_eq(first, image))
                }),
                "simultaneous cache misses must reuse one decoded bitmap"
            );
        });
    }

    #[gpui::test]
    fn idle_preview_reopening_schedules_fresh_decoder_work(cx: &mut TestAppContext) {
        let diagram = small_diagram();
        let first = cx.new(|cx| DiagramImage::new(diagram.clone(), cx));
        first.update(cx, |image, cx| {
            image.set_display_size(300.0, 100.0, 1.0, cx)
        });
        finish_decoding(cx);
        let (previous_id, pixels) = cx.read(|cx| {
            let image = first.read(cx).image.as_ref().unwrap();
            (image.id, Arc::downgrade(image))
        });
        drop(first);
        cx.update(|_| ());
        let capacity = cx.update(crate::diagram_work::raster);
        let permit = crate::diagram_work::occupy(&capacity);
        let reopened = cx.new(|cx| DiagramImage::new(diagram, cx));
        reopened.update(cx, |image, cx| {
            image.set_display_size(300.0, 100.0, 1.0, cx)
        });
        cx.read(|cx| {
            let reopened = reopened.read(cx);
            assert!(
                reopened.task.is_some(),
                "a retired bitmap must request new decoder work"
            );
            assert!(
                reopened.image.is_none(),
                "no idle pixels may survive closing the original view"
            );
        });
        assert!(pixels.upgrade().is_none());
        drop(permit);
        finish_decoding(cx);
        assert_ne!(
            cx.read(|cx| reopened.read(cx).decoded_id()),
            Some(previous_id)
        );
        assert!(cx.read(|cx| reopened.read(cx).decoded_size().is_some()));
    }

    #[gpui::test]
    fn diagram_pixels_wait_for_shared_capacity_and_cancel_obsolete_sizes(cx: &mut TestAppContext) {
        let queue = cx.update(crate::diagram_work::raster);
        let permit = crate::diagram_work::occupy(&queue);
        let diagram = small_diagram();
        let previews = (0..8)
            .map(|_| {
                let view = cx.new(|cx| DiagramImage::new(diagram.clone(), cx));
                view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
                view
            })
            .collect::<Vec<_>>();
        let viewer = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        finish_decoding(cx);
        cx.read(|cx| {
            assert!(
                previews
                    .iter()
                    .all(|view| view.read(cx).decoded_size().is_none()),
                "previews must wait asynchronously when the shared raster worker is busy"
            );
            assert!(
                viewer.read(cx).decoded_size().is_none(),
                "viewer must use the same limit"
            );
        });
        let window = cx.add_empty_window();
        window.update(|window, cx| viewer.update(cx, |image, cx| image.unload(window, cx)));
        previews[0].update(cx, |view, cx| view.set_display_size(400.0, 200.0, 1.0, cx));
        drop(permit);
        finish_decoding(cx);
        cx.read(|cx| {
            assert_eq!(
                previews[0].read(cx).decoded_size(),
                Some(RasterSize {
                    width: 400,
                    height: 200
                })
            );
            assert!(
                previews
                    .iter()
                    .all(|view| view.read(cx).decoded_size().is_some())
            );
            assert!(
                viewer.read(cx).decoded_size().is_none(),
                "closed viewer must stay unloaded"
            );
        });
        assert!(
            crate::diagram_work::occupy(&queue).len() == crate::diagram_work::MAX_CONCURRENT,
            "all raster capacity must be returned"
        );
    }

    #[gpui::test]
    fn fixed_viewer_bitmap_survives_zoom_pan_resize_and_dpi_changes(cx: &mut TestAppContext) {
        let diagram = std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT")
            .map(|path| {
                let text = std::fs::read_to_string(path).expect("read diagram document");
                let lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
                let blocks = crate::code_blocks::collect(&lines).0;
                let block = blocks
                    .iter()
                    .find(|block| block.is_mermaid())
                    .expect("first Mermaid diagram");
                diagrams::render(&DiagramKey {
                    source: block.source.clone(),
                    dark: false,
                })
                .unwrap()
            })
            .unwrap_or_else(small_diagram);
        let target = RasterSize::for_viewer(diagram.width, diagram.height);
        let image = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        image.update(cx, |image, cx| {
            image.set_viewport((600.0, 400.0), (0.0, 0.0), 1.0, 1.0, cx)
        });
        finish_decoding(cx);
        let original = cx.read(|cx| image.read(cx).image.clone().unwrap());
        assert_eq!(original.as_bytes(0).unwrap().len(), target.bytes());
        assert!(target.bytes() <= 64 * 1024 * 1024);
        println!(
            "fixed_viewer_pixels={}x{} pixel_mib={:.2}",
            target.width,
            target.height,
            target.bytes() as f64 / 1048576.0
        );
        for (size, origin, scale, dpi) in [
            ((600.0, 400.0), (-8000.0, -1000.0), 1.0, 1.0),
            ((600.0, 400.0), (-300.0, -200.0), 8.0, 1.0),
            ((1400.0, 900.0), (500.0, 400.0), 0.05, 2.0),
            ((1200.0, 800.0), (100.0, 50.0), 2.0, 1.5),
        ] {
            image.update(cx, |image, cx| {
                image.set_viewport(size, origin, scale, dpi, cx)
            });
            finish_decoding(cx);
            cx.read(|cx| {
                let image = image.read(cx);
                assert!(
                    Arc::ptr_eq(&original, image.image.as_ref().unwrap()),
                    "all viewer gestures must keep the original full-diagram bitmap"
                );
                assert!(
                    image.task.is_none(),
                    "viewer gestures must never schedule another decode"
                );
            });
        }
        // Even a caller changing display dimensions must preserve the fixed target.
        image.update(cx, |image, cx| {
            image.set_display_size(4000.0, 8000.0, 4.0, cx)
        });
        finish_decoding(cx);
        assert!(cx.read(|cx| Arc::ptr_eq(&original, image.read(cx).image.as_ref().unwrap())));
    }

    #[gpui::test]
    fn opening_decodes_the_complete_diagram_once_without_a_viewport(cx: &mut TestAppContext) {
        let diagram = Diagram {
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="500" viewBox="0 0 2000 500"><rect width="1000" height="500" fill="#ff0000"/><rect x="1000" width="1000" height="500" fill="#0000ff"/></svg>"##.to_vec())),
            width: 2000.0,
            height: 500.0,
        };
        let image = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        finish_decoding(cx);
        cx.read(|cx| {
            let image = image.read(cx);
            assert_eq!(
                image.decoded_size(),
                Some(RasterSize {
                    width: 8000,
                    height: 2000
                })
            );
            let image = image.image.as_ref().unwrap();
            let size = image.size(0);
            let pixel = |x| {
                let index = ((size.height.0 / 2 * size.width.0 + x) * 4) as usize;
                &image.as_bytes(0).unwrap()[index..index + 4]
            };
            assert_ne!(
                pixel(1000),
                pixel(7000),
                "both ends of the full diagram must exist in one bitmap"
            );
        });
    }

    #[gpui::test]
    fn closing_cancels_pending_high_definition_work_and_prevents_reallocation(
        cx: &mut TestAppContext,
    ) {
        let image = cx.new(|cx| DiagramImage::high_definition(small_diagram(), cx));
        let window = cx.add_empty_window();
        // Cancel before the initial decode gets an executor turn.
        window.update(|window, cx| image.update(cx, |image, cx| image.unload(window, cx)));
        finish_decoding(cx);
        image.update(cx, |image, cx| {
            image.set_viewport((600.0, 400.0), (0.0, 0.0), 1.0, 1.0, cx);
            image.set_display_size(2000.0, 1000.0, 2.0, cx);
        });
        finish_decoding(cx);
        cx.read(|cx| {
            let image = image.read(cx);
            assert!(image.decoded_size().is_none());
            assert!(image.task.is_none());
            assert!(image.requested.is_none());
        });
    }

    #[gpui::test]
    fn closing_releases_the_fixed_bitmap_and_reopening_allocates_a_fresh_one(
        cx: &mut TestAppContext,
    ) {
        let diagram = small_diagram();
        let image = cx.new(|cx| DiagramImage::high_definition(diagram.clone(), cx));
        finish_decoding(cx);
        let weak = cx.read(|cx| Arc::downgrade(image.read(cx).image.as_ref().unwrap()));
        let window = cx.add_empty_window();
        window.update(|window, cx| image.update(cx, |image, cx| image.unload(window, cx)));
        assert!(
            weak.upgrade().is_none(),
            "close must release the full bitmap immediately"
        );
        let reopened = cx.new(|cx| DiagramImage::high_definition(diagram, cx));
        finish_decoding(cx);
        assert!(cx.read(|cx| reopened.read(cx).decoded_size().is_some()));
        let weak = cx.read(|cx| Arc::downgrade(reopened.read(cx).image.as_ref().unwrap()));
        drop(reopened);
        cx.update(|_| ());
        assert!(
            weak.upgrade().is_none(),
            "entity release must also evict its bitmap"
        );
    }

    #[gpui::test]
    fn resize_releases_previous_targets_and_revisiting_decodes_fresh_pixels(
        cx: &mut TestAppContext,
    ) {
        let view = cx.new(|cx| DiagramImage::new(small_diagram(), cx));
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        finish_decoding(cx);
        let first = cx.read(|cx| view.read(cx).image.clone().unwrap());
        let first_id = first.id;
        let weak = Arc::downgrade(&first);
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        assert!(cx.read(|cx| Arc::ptr_eq(&first, view.read(cx).image.as_ref().unwrap())));
        drop(first);
        view.update(cx, |view, cx| view.set_display_size(200.0, 70.0, 2.0, cx));
        finish_decoding(cx);
        assert!(
            weak.upgrade().is_none(),
            "release the replaced target without retaining an idle bitmap"
        );
        let resized = cx.read(|cx| Arc::downgrade(view.read(cx).image.as_ref().unwrap()));
        let capacity = cx.update(crate::diagram_work::raster);
        let permit = crate::diagram_work::occupy(&capacity);
        view.update(cx, |view, cx| view.set_display_size(300.0, 100.0, 1.0, cx));
        cx.read(|cx| {
            let view = view.read(cx);
            assert!(
                view.task.is_some(),
                "restoring a retired resolution must decode it again"
            );
            assert!(Arc::ptr_eq(
                &resized.upgrade().unwrap(),
                view.image.as_ref().unwrap()
            ));
        });
        drop(permit);
        finish_decoding(cx);
        assert!(resized.upgrade().is_none());
        assert_ne!(cx.read(|cx| view.read(cx).decoded_id()), Some(first_id));
        let reopened = cx.read(|cx| Arc::downgrade(view.read(cx).image.as_ref().unwrap()));
        drop(view);
        cx.update(|_| ());
        cx.run_until_parked();
        assert!(weak.upgrade().is_none());
        assert!(resized.upgrade().is_none());
        assert!(reopened.upgrade().is_none());
    }
}
