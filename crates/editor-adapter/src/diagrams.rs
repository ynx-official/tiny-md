//! Native Mermaid → SVG. GPUI decodes the SVG off the UI thread.
use crate::diagram_image::DiagramImage;
use crate::mermaid_parse::parse;
use gpui::{
    App, AppContext, Div, Entity, Hsla, Image, ImageFormat, ScrollHandle, Task, div, point,
    prelude::*, px,
};
use mermaid_rs_renderer::{LayoutConfig, Theme, measure_svg_dimensions};
use std::sync::Arc;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct DiagramKey {
    pub source: String,
    pub dark: bool,
}

#[derive(Debug, Clone)]
pub struct Diagram {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
}

pub const PREVIEW_PADDING: f32 = 24.0;

#[derive(Default)]
pub struct DiagramView {
    /// None fits the available width; explicit zoom is relative to SVG units.
    pub zoom: Option<f32>,
    pub scroll: ScrollHandle,
    preview: Option<Entity<DiagramImage>>,
    space: Option<(String, (f32, f32))>,
    pub(crate) show_error_source: bool,
}

impl DiagramView {
    pub fn reserve_space(
        &mut self,
        source: &str,
        diagram: Option<&Diagram>,
        error: bool,
        locked: bool,
    ) {
        if self
            .space
            .as_ref()
            .is_none_or(|(previous, _)| previous != source)
        {
            // Cheap source-based estimate only; exact layout stays off the UI thread.
            let lines = source.lines().count().clamp(1, 200) as f32;
            let horizontal = source
                .lines()
                .next()
                .is_some_and(|line| line.contains(" LR") || line.contains(" RL"));
            let height = if horizontal {
                (lines * 24.0).clamp(240.0, 1200.0)
            } else {
                (lines * 64.0).clamp(240.0, 3200.0)
            };
            self.space = Some((source.to_owned(), (600.0, height)));
            self.show_error_source = false;
        }
        if !locked {
            self.show_error_source = error;
            if let Some(diagram) = diagram {
                self.space.as_mut().unwrap().1 = (diagram.width, diagram.height);
            } else if error {
                self.space.as_mut().unwrap().1 = (600.0, 72.0);
            }
        }
    }

    pub fn reserved_height(&self, available: f32) -> f32 {
        if self.show_error_source {
            return 96.0;
        }
        let (_, (width, height)) = self
            .space
            .as_ref()
            .expect("diagram reserves space before layout");
        height
            * self
                .zoom
                .unwrap_or_else(|| (available.max(1.0) / width).min(2.0))
            + PREVIEW_PADDING
    }

    #[cfg(test)]
    pub(crate) fn preview_id(&self, cx: &App) -> Option<gpui::ImageId> {
        self.preview
            .as_ref()
            .and_then(|preview| preview.read(cx).decoded_id())
    }

    #[cfg(test)]
    pub(crate) fn preview_size(&self, cx: &App) -> Option<RasterSize> {
        self.preview
            .as_ref()
            .and_then(|preview| preview.read(cx).decoded_size())
    }

    #[cfg(test)]
    pub(crate) fn preview_reference(&self, cx: &App) -> Option<std::sync::Weak<gpui::RenderImage>> {
        self.preview
            .as_ref()
            .and_then(|preview| preview.read(cx).decoded_reference())
    }
    pub fn retain_preview(&mut self, diagram: Option<&Diagram>, cx: &App) {
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| diagram.is_none_or(|diagram| !preview.read(cx).is_for(diagram)))
        {
            self.preview = None;
        }
    }

    pub fn preview(
        &mut self,
        diagram: &Diagram,
        width: f32,
        height: f32,
        dpi: f32,
        cx: &mut App,
    ) -> Entity<DiagramImage> {
        if self
            .preview
            .as_ref()
            .is_none_or(|preview| !preview.read(cx).is_for(diagram))
        {
            self.preview = Some(cx.new(|cx| DiagramImage::new(diagram.clone(), cx)));
        }
        let preview = self.preview.as_ref().unwrap().clone();
        preview.update(cx, |preview, cx| {
            preview.set_display_size(width, height, dpi, cx)
        });
        preview
    }

    pub fn set_zoom(&mut self, zoom: Option<f32>) {
        self.zoom = zoom.map(|zoom| zoom.clamp(0.25, 3.0));
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }
}

/// Static, theme-colored placeholders avoid adding animation work during loading.
pub(crate) fn skeleton(surface: Hsla, text: Hsla) -> Div {
    div()
        .debug_selector(|| "diagram-skeleton".into())
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .text_size(px(12.0))
        .text_color(text)
        .child(
            div()
                .w_full()
                .max_w(px(112.0))
                .h(px(28.0))
                .rounded(px(6.0))
                .bg(surface),
        )
        .child(div().w(px(2.0)).h(px(16.0)).bg(surface))
        .child(
            div()
                .w_full()
                .max_w(px(160.0))
                .h(px(32.0))
                .rounded(px(6.0))
                .bg(surface),
        )
        .child(div().w(px(2.0)).h(px(16.0)).bg(surface))
        .child(
            div()
                .w_full()
                .max_w(px(112.0))
                .h(px(28.0))
                .rounded(px(6.0))
                .bg(surface),
        )
        .child("正在生成流程图…")
}

impl Diagram {
    /// Only resize the SVG root canvas. Its viewBox and vector geometry stay intact.
    pub fn image_at_size(&self, size: RasterSize) -> Arc<Image> {
        let svg = std::str::from_utf8(self.image.bytes()).expect("renderer produces UTF-8 SVG");
        let start = svg.find("<svg ").expect("renderer produces an SVG root");
        let end = start + svg[start..].find('>').unwrap();
        let root = replace_root_attribute(&svg[start..end], "width", size.width);
        let root = replace_root_attribute(&root, "height", size.height);
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            format!("{}{}{}", &svg[..start], root, &svg[end..]).into_bytes(),
        ))
    }

    pub fn display_scale(&self, available: f32, zoom: Option<f32>) -> f32 {
        zoom.unwrap_or_else(|| (available.max(1.0) / self.width).min(2.0))
    }

    pub fn display_size(&self, available: f32, zoom: Option<f32>) -> (f32, f32) {
        let scale = self.display_scale(available, zoom);
        (self.width * scale, self.height * scale)
    }

    pub fn viewport_height(&self, available: f32, zoom: Option<f32>) -> f32 {
        self.display_size(available, zoom).1 + PREVIEW_PADDING
    }
}

fn replace_root_attribute(root: &str, name: &str, value: u32) -> String {
    let marker = format!(" {name}=\"");
    let start = root
        .find(&marker)
        .expect("renderer supplies canvas dimensions")
        + marker.len();
    let end = start + root[start..].find('"').unwrap();
    format!("{}{value}{}", &root[..start], &root[end..])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RasterSize {
    pub width: u32,
    pub height: u32,
}

impl RasterSize {
    /// One fixed full-diagram bitmap; viewer gestures never change this target.
    pub fn for_viewer(width: f32, height: f32) -> Self {
        let sanitize = |value: f32| {
            if value.is_finite() && value > 0.0 {
                value
            } else {
                1.0
            }
        };
        let width = sanitize(width) * 4.0;
        let height = sanitize(height) * 4.0;
        // Both GPUI's DirectX and Metal atlases limit texture edges to 16384.
        // The 64 MiB pixel budget also limits square diagrams to 4096 x 4096.
        let scale = (16384.0 / width)
            .min(16384.0 / height)
            .min((16_777_216.0 / (width * height)).sqrt())
            .min(1.0);
        Self {
            width: (width * scale).floor().max(1.0) as u32,
            height: (height * scale).floor().max(1.0) as u32,
        }
    }

    pub fn for_display(width: f32, height: f32, dpi: f32) -> Self {
        let sanitize = |value: f32| {
            if value.is_finite() && value > 0.0 {
                value
            } else {
                1.0
            }
        };
        let width = sanitize(width) * sanitize(dpi).min(4.0);
        let height = sanitize(height) * sanitize(dpi).min(4.0);
        // Preserve the existing per-image safety bound, including at large viewer zoom.
        let scale = (4096.0 / width)
            .min(8192.0 / height)
            .min((16_777_216.0 / (width * height)).sqrt())
            .min(1.0);
        Self {
            width: (width * scale).floor().max(1.0) as u32,
            height: (height * scale).floor().max(1.0) as u32,
        }
    }

    pub fn bytes(self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

pub enum DiagramState {
    Loading { _task: Task<()> },
    Ready(Diagram),
    Error(String),
}

pub fn render(key: &DiagramKey) -> Result<Diagram, String> {
    if key.source.len() > 64 * 1024 {
        return Err("流程图源码过长，请拆分为多个图。".into());
    }
    let parsed = parse(&key.source)?;
    if parsed.graph.kind == mermaid_rs_renderer::DiagramKind::Flowchart
        && parsed.graph.nodes.is_empty()
    {
        return Err("未找到有效节点，请检查节点括号和连接语法。".into());
    }
    if parsed.graph.nodes.len() > 200 {
        return Err("流程图超过 200 个节点，请拆分后预览。".into());
    }
    let mut theme = if key.dark {
        Theme::dark()
    } else {
        Theme::modern()
    };
    // Resolve Chinese with an installed platform face before USVG has to scan
    // the system font database for a fallback on every shaped text run.
    theme.font_family = if cfg!(target_os = "windows") {
        "Microsoft YaHei, Segoe UI, sans-serif"
    } else {
        "Helvetica Neue, PingFang SC, sans-serif"
    }
    .into();
    theme.font_size = 16.0;
    theme.background = "transparent".into();
    theme.edge_label_background = "transparent".into();
    if key.dark {
        theme.primary_color = "#293C32".into();
        theme.secondary_color = "#31453B".into();
        theme.tertiary_color = "#293C32".into();
        theme.primary_text_color = "#E6F1EA".into();
        theme.text_color = "#DCE9E1".into();
        theme.primary_border_color = "#799F8B".into();
        theme.line_color = "#A2BEAF".into();
    } else {
        theme.primary_color = "#EDF6F1".into();
        theme.primary_border_color = "#8CAE9D".into();
        theme.line_color = "#617F70".into();
    }
    let config = LayoutConfig::default();
    let (layout, backend) =
        crate::diagram_layout::layout(&parsed.graph, &theme, &config, parsed.init_config.as_ref());
    let dimensions = measure_svg_dimensions(&layout, &config, None);
    if !dimensions.width.is_finite()
        || !dimensions.height.is_finite()
        || dimensions.width <= 0.0
        || dimensions.height <= 0.0
        // ELK can produce valid tall compound graphs. The vector canvas does
        // not allocate pixels; both preview and viewer still enforce 64 MiB.
        || dimensions.width > 128000.0
        || dimensions.height > 128000.0
    {
        return Err("流程图尺寸过大，无法预览。".into());
    }
    // Keep vector geometry at logical size; each view chooses its own pixel canvas.
    let svg = mermaid_rs_renderer::render::render_svg_with_dimensions(
        &layout,
        &theme,
        &config,
        Some((dimensions.width, dimensions.height)),
    );
    // The marker is diagnostic metadata; it never appears in the editor UI.
    let svg = svg.replacen("<svg ", &format!("<svg data-layout=\"{backend:?}\" "), 1);
    Ok(Diagram {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        width: dimensions.width,
        height: dimensions.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_transfer_flowchart_with_uppercase_end_and_line_breaks_renders() {
        let source = include_str!("../../../fixtures/diagrams/task-transfer.mmd");
        let parsed = parse(source).expect("the complete task-transfer source is a valid flowchart");
        assert_eq!(
            parsed.graph.nodes.len(),
            19,
            "nodes={:?}; edges={:?}",
            parsed.graph.nodes.keys().collect::<Vec<_>>(),
            parsed
                .graph
                .edges
                .iter()
                .map(|edge| (&edge.from, &edge.to))
                .collect::<Vec<_>>()
        );
        assert_eq!(parsed.graph.edges.len(), 24);
        assert!(parsed.graph.nodes.contains_key("END"));
        for dark in [false, true] {
            let diagram = render(&DiagramKey {
                source: source.into(),
                dark,
            })
            .expect("valid task-transfer source must render");
            let (width, height) = diagram.display_size(1000.0, None);
            let pixels = crate::diagram_raster::decode(
                &diagram,
                RasterSize::for_display(width, height, 2.0),
            )
            .unwrap();
            assert!(
                pixels
                    .as_bytes(0)
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] != 0)
            );
        }
    }

    #[test]
    #[ignore = "set TINY_MD_DIAGRAM_DOCUMENT to profile SVG decoding"]
    fn profile_svg_decode_phases() {
        let text =
            std::fs::read_to_string(std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT").unwrap()).unwrap();
        let (blocks, _) =
            crate::code_blocks::collect(&text.lines().map(str::to_owned).collect::<Vec<_>>());
        let index = std::env::var("TINY_MD_DIAGRAM_SNAPSHOT_INDEX")
            .ok()
            .map(|value| value.parse::<usize>().unwrap())
            .unwrap_or(2);
        assert!(index > 0);
        let block = blocks
            .iter()
            .filter(|block| block.is_mermaid())
            .nth(index - 1)
            .unwrap();
        let diagram = render(&DiagramKey {
            source: block.source.clone(),
            dark: std::env::var("TINY_MD_DIAGRAM_SNAPSHOT_DARK").is_ok_and(|value| value == "1"),
        })
        .unwrap();
        let (width, height) = diagram.display_size(1000.0, None);
        let size = RasterSize::for_display(width, height, 2.0);
        let started = std::time::Instant::now();
        let options = crate::diagram_raster::options();
        println!(
            "font_prepare_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let image = diagram.image_at_size(size);
        let started = std::time::Instant::now();
        let tree = usvg::Tree::from_data(image.bytes(), options).unwrap();
        println!(
            "svg_parse_and_shape_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = std::time::Instant::now();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width, size.height).unwrap();
        println!(
            "pixel_allocate_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = std::time::Instant::now();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        println!(
            "raster_paint_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        if let Some(directory) = std::env::var_os("TINY_MD_DIAGRAM_SNAPSHOT_DIRECTORY") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("elk-large.svg"), image.bytes()).unwrap();
            pixmap.save_png(directory.join("elk-large.png")).unwrap();
        }
    }

    #[test]
    fn long_elk_layouts_keep_the_existing_pixel_budget() {
        let source = format!(
            "flowchart TD\n{}",
            (0..159)
                .map(|index| format!("N{index} --> N{}", index + 1))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let diagram = render(&DiagramKey {
            source,
            dark: false,
        })
        .expect("valid long ELK diagrams must remain previewable");
        assert!(diagram.height > 12000.0);
        let (width, height) = diagram.display_size(1000.0, None);
        let pixels = RasterSize::for_display(width, height, 2.0);
        assert!(pixels.width <= 4096 && pixels.height <= 8192);
        assert!(pixels.bytes() <= 64 * 1024 * 1024);
    }

    /// Opt-in measurement of the app's layout and SVG decode paths. Timings
    /// are diagnostic output, not wall-clock assertions or native-window FPS.
    #[test]
    #[ignore = "set TINY_MD_DIAGRAM_DOCUMENT to profile a local document"]
    fn profile_diagram_generation() {
        let path =
            std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT").expect("set TINY_MD_DIAGRAM_DOCUMENT");
        let text = std::fs::read_to_string(path).expect("read profile document");
        let lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        let (blocks, _) = crate::code_blocks::collect(&lines);
        let limit = std::env::var("TINY_MD_DIAGRAM_PROFILE_LIMIT")
            .ok()
            .map(|value| value.parse::<usize>().expect("positive profile limit"))
            .unwrap_or(usize::MAX);
        let workers = std::env::var("TINY_MD_DIAGRAM_PROFILE_WORKERS")
            .ok()
            .map(|value| value.parse::<usize>().expect("worker count"))
            .unwrap_or(1);
        assert!((1..=crate::diagram_work::MAX_CONCURRENT).contains(&workers));
        let blocks = blocks
            .into_iter()
            .filter(|block| block.is_mermaid())
            .take(limit)
            .collect::<Vec<_>>();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let started = std::time::Instant::now();
        let totals = std::thread::scope(|scope| {
            let mut tasks = Vec::new();
            for _ in 0..workers {
                let (blocks, next) = (&blocks, &next);
                tasks.push(scope.spawn(move || {
                    let mut totals = (0.0, 0.0, 0usize, 0, 0);
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(block) = blocks.get(index) else {
                            break;
                        };
                        let result =
                            profile_one_diagram(block, index, crate::diagram_raster::decode);
                        totals.0 += result.0;
                        totals.1 += result.1;
                        totals.2 += result.2;
                        totals.3 += result.3;
                        totals.4 += result.4;
                    }
                    totals
                }));
            }
            tasks
                .into_iter()
                .fold((0.0, 0.0, 0usize, 0, 0), |mut total, task| {
                    let result = task.join().expect("diagram profiling worker");
                    total.0 += result.0;
                    total.1 += result.1;
                    total.2 += result.2;
                    total.3 += result.3;
                    total.4 += result.4;
                    total
                })
        });
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let (layout_ms, pixels_ms, bytes, ready, errors) = totals;
        println!(
            "diagram_profile_summary workers={workers} elapsed_ms={elapsed_ms:.3} ready={ready} errors={errors} layout_ms={layout_ms:.3} pixels_ms={pixels_ms:.3} pixel_mib={:.2} debug_assertions={}",
            bytes as f64 / (1024.0 * 1024.0),
            cfg!(debug_assertions)
        );
        assert!(ready > 0, "profile document must contain a valid diagram");
    }

    fn profile_one_diagram(
        block: &crate::code_blocks::CodeBlock,
        index: usize,
        decode: impl FnOnce(&Diagram, RasterSize) -> Result<Arc<gpui::RenderImage>, String>,
    ) -> (f64, f64, usize, usize, usize) {
        let started = std::time::Instant::now();
        let result = render(&DiagramKey {
            source: block.source.clone(),
            dark: false,
        });
        let layout = started.elapsed().as_secs_f64() * 1000.0;
        let diagram = match result {
            Ok(diagram) => diagram,
            Err(_) => {
                println!(
                    "diagram_profile index={} layout_ms={layout:.3} error=true",
                    index + 1
                );
                return (layout, 0.0, 0, 0, 1);
            }
        };
        let svg = std::str::from_utf8(diagram.image.bytes()).unwrap();
        let backend = svg
            .split("data-layout=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        // Repeat only parsing after the real cold generation to locate
        // parser cost without warming its caches before that measurement.
        let started = std::time::Instant::now();
        let parsed = parse(&block.source).expect("already rendered source");
        let parse_repeat_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "diagram_parse_probe index={} parse_repeat_ms={parse_repeat_ms:.3} nodes={} edges={}",
            index + 1,
            parsed.graph.nodes.len(),
            parsed.graph.edges.len()
        );
        let (width, height) = diagram.display_size(1000.0, None);
        let size = RasterSize::for_display(width, height, 2.0);
        let started = std::time::Instant::now();
        let image = decode(&diagram, size).expect("decode profile diagram");
        let pixels = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "diagram_profile index={} backend={backend} layout_ms={layout:.3} pixels_ms={pixels:.3} pixels={}x{}",
            index + 1,
            size.width,
            size.height
        );
        (layout, pixels, image.as_bytes(0).unwrap().len(), 1, 0)
    }

    #[test]
    fn generated_svg_does_not_preallocate_a_three_times_larger_canvas() {
        let diagram = render(&DiagramKey {
            source: "flowchart TD\nA[开始] --> B[处理] --> C[完成]".into(),
            dark: false,
        })
        .unwrap();
        let svg = std::str::from_utf8(diagram.image.bytes()).unwrap();
        let width = svg
            .split("width=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap();
        assert!(
            (width - diagram.width).abs() < 1.0,
            "SVG must retain logical dimensions until the actual display size is known: {width} vs {}",
            diagram.width
        );
    }

    #[test]
    fn renders_chinese_branches_in_both_themes() {
        for dark in [false, true] {
            let diagram = render(&DiagramKey { source: "flowchart LR\n A[开始] --> B{保存？}\n B -->|是| C[完成]\n B -->|否| D[继续编辑]".into(), dark }).unwrap();
            assert!(diagram.width > 100.0);
            assert!(diagram.height > 50.0);
            let (width, height) = diagram.display_size(300.0, None);
            assert!(width <= 300.0);
            assert!((width / height - diagram.width / diagram.height).abs() < 0.01);
        }
    }

    #[test]
    fn long_diagrams_expand_preview_to_full_height_at_every_scale() {
        let diagram = Diagram {
            image: Arc::new(Image::from_bytes(ImageFormat::Svg, Vec::new())),
            width: 400.0,
            height: 2400.0,
        };
        assert_eq!(diagram.display_size(800.0, None), (800.0, 4800.0));
        assert_eq!(diagram.viewport_height(800.0, None), 4824.0);
        assert_eq!(diagram.display_size(800.0, Some(1.0)), (400.0, 2400.0));
        assert_eq!(diagram.viewport_height(800.0, Some(1.0)), 2424.0);
        assert_eq!(diagram.display_size(300.0, Some(1.5)), (600.0, 3600.0));
        assert_eq!(diagram.viewport_height(300.0, Some(1.5)), 3624.0);
        assert_eq!(diagram.display_size(300.0, None), (300.0, 1800.0));
        assert_eq!(diagram.viewport_height(300.0, None), 1824.0);
    }

    #[test]
    fn fixed_viewer_resolution_is_four_times_logical_size_and_bounds_full_diagrams() {
        assert_eq!(
            RasterSize::for_viewer(400.0, 200.0),
            RasterSize {
                width: 1600,
                height: 800
            }
        );
        assert_eq!(
            RasterSize::for_viewer(200.0, 400.0),
            RasterSize {
                width: 800,
                height: 1600
            }
        );
        for (width, height) in [
            (12000.0, 2000.0),
            (2000.0, 12000.0),
            (12000.0, 12000.0),
            (400.0, 12000.0),
            (12000.0, 400.0),
        ] {
            let size = RasterSize::for_viewer(width, height);
            assert!(size.width <= 16384 && size.height <= 16384);
            assert!(size.bytes() <= 64 * 1024 * 1024);
            assert!((size.width as f32 / size.height as f32 - width / height).abs() < 0.1);
            let transposed = RasterSize::for_viewer(height, width);
            assert_eq!(size.width, transposed.height);
            assert_eq!(size.height, transposed.width);
        }
        for (width, height) in [(f32::NAN, 0.0), (f32::INFINITY, -20.0), (0.0, 0.0)] {
            let size = RasterSize::for_viewer(width, height);
            assert!(size.width > 0 && size.height > 0);
            assert!(size.bytes() <= 64 * 1024 * 1024);
        }
    }

    #[test]
    fn raster_resolution_follows_display_size_and_bounds_allocation() {
        assert_eq!(
            RasterSize::for_display(400.0, 2400.0, 1.0),
            RasterSize {
                width: 400,
                height: 2400
            }
        );
        assert_eq!(
            RasterSize::for_display(400.0, 2400.0, 2.0),
            RasterSize {
                width: 800,
                height: 4800
            }
        );
        for (width, height) in [(12000.0, 12000.0), (400.0, 12000.0), (12000.0, 400.0)] {
            let size = RasterSize::for_display(width, height, 2.0);
            assert!(size.width <= 4096);
            assert!(size.height <= 8192);
            assert!(size.bytes() <= 64 * 1024 * 1024);
        }
    }

    #[gpui::test]
    fn requested_canvas_is_actually_decoded_at_display_resolution(cx: &mut gpui::TestAppContext) {
        let diagram = render(&DiagramKey {
            source: "flowchart LR\n A[开始] --> B[结束]".into(),
            dark: false,
        })
        .unwrap();
        let (width, height) = diagram.display_size(300.0, None);
        let size = RasterSize::for_display(width, height, 1.5);
        let image = cx.update(|cx| {
            diagram
                .image_at_size(size)
                .to_image_data(cx.svg_renderer())
                .unwrap()
        });
        assert_eq!(image.as_bytes(0).unwrap().len(), size.bytes());
        assert_eq!(image.size(0).width.0 as u32, size.width);
        assert_eq!(image.size(0).height.0 as u32, size.height);
    }

    #[test]
    fn rejects_invalid_and_oversized_source() {
        assert!(
            render(&DiagramKey {
                source: "flowchart LR\nA[未闭合 --> B".into(),
                dark: false
            })
            .is_err()
        );
        assert!(
            render(&DiagramKey {
                source: "this is not mermaid".into(),
                dark: false
            })
            .is_err()
        );
        assert!(
            render(&DiagramKey {
                source: "x".repeat(65537),
                dark: false
            })
            .is_err()
        );
    }
}
