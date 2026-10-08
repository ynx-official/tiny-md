//! Native Mermaid → SVG. GPUI decodes the SVG off the UI thread.
use crate::diagram_image::DiagramImage;
use gpui::{App, AppContext, Entity, Image, ImageFormat, ScrollHandle, Task, point, px};
use mermaid_rs_renderer::{
    LayoutConfig, Theme, compute_layout, measure_svg_dimensions, parse_mermaid_strict,
};
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
}

impl DiagramView {
    #[cfg(test)]
    pub(crate) fn preview_size(&self, cx: &App) -> Option<RasterSize> {
        self.preview
            .as_ref()
            .and_then(|preview| preview.read(cx).decoded_size())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    let parsed = parse_mermaid_strict(&key.source).map_err(|e| e.to_string())?;
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
    theme.font_family = "Helvetica Neue, PingFang SC, sans-serif".into();
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
    let layout = compute_layout(&parsed.graph, &theme, &config);
    let dimensions = measure_svg_dimensions(&layout, &config, None);
    if !dimensions.width.is_finite()
        || !dimensions.height.is_finite()
        || dimensions.width <= 0.0
        || dimensions.height <= 0.0
        || dimensions.width > 12000.0
        || dimensions.height > 12000.0
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
