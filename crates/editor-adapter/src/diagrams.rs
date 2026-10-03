//! Native Mermaid → SVG. GPUI decodes the SVG off the UI thread.
use gpui::{Image, ImageFormat, ScrollHandle, Task, point, px};
use mermaid_rs_renderer::{
    LayoutConfig, Theme, compute_layout, measure_svg_dimensions, parse_mermaid_strict,
};
use std::sync::Arc;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct DiagramKey {
    pub source: String,
    pub dark: bool,
}

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
}

impl DiagramView {
    pub fn set_zoom(&mut self, zoom: Option<f32>) {
        self.zoom = zoom.map(|zoom| zoom.clamp(0.25, 3.0));
        self.scroll.set_offset(point(px(0.0), px(0.0)));
    }
}

impl Diagram {
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
    // Keep long diagrams sharp at readable zoom, while bounding decoded RGBA
    // allocation to 64 MiB. The viewBox preserves the original graph geometry.
    let raster_scale = raster_scale(dimensions.width, dimensions.height);
    let svg = mermaid_rs_renderer::render::render_svg_with_dimensions(
        &layout,
        &theme,
        &config,
        Some((
            dimensions.width * raster_scale,
            dimensions.height * raster_scale,
        )),
    );
    Ok(Diagram {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        width: dimensions.width,
        height: dimensions.height,
    })
}

fn raster_scale(width: f32, height: f32) -> f32 {
    (4096.0 / width)
        .min(8192.0 / height)
        .min((16_777_216.0 / (width * height)).sqrt())
        .min(3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn raster_resolution_preserves_long_diagrams_with_bounded_allocation() {
        assert_eq!(raster_scale(400.0, 2400.0), 3.0);
        for (width, height) in [(12000.0, 12000.0), (400.0, 12000.0), (12000.0, 400.0)] {
            let scale = raster_scale(width, height);
            assert!(width * scale <= 4096.0);
            assert!(height * scale <= 8192.0);
            assert!(width * height * scale * scale <= 16_777_218.0);
        }
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
