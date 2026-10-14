//! Use the existing USVG/RESVG pipeline with bounded shared font bytes.
use crate::diagrams::{Diagram, RasterSize};
use gpui::RenderImage;
use std::io::Read;
use std::sync::{Arc, LazyLock};
use usvg::fontdb::{Database, Family, Query, Source, Style, Weight};

const FONT_BYTES_LIMIT: usize = 64 * 1024 * 1024;
const FONT_FAMILIES: &[&str] = if cfg!(target_os = "windows") {
    &["Microsoft YaHei", "Segoe UI"]
} else {
    &["Helvetica Neue", "PingFang SC"]
};

fn retain_font_bytes(db: &mut Database, budget: usize) -> usize {
    let mut paths = std::collections::BTreeSet::new();
    for family in FONT_FAMILIES {
        for weight in [Weight::NORMAL, Weight::BOLD] {
            for style in [Style::Normal, Style::Italic] {
                let query = Query {
                    families: &[Family::Name(family)],
                    weight,
                    style,
                    ..Default::default()
                };
                if let Some(id) = db.query(&query)
                    && let Some((Source::File(path), _)) = db.face_source(id)
                {
                    paths.insert(path);
                }
            }
        }
    }
    let mut retained = 0;
    for path in paths {
        let remaining = budget - retained;
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        if file
            .metadata()
            .ok()
            .is_none_or(|metadata| metadata.len() > remaining as u64)
        {
            continue;
        }
        let mut data = Vec::new();
        if file
            .take(remaining as u64 + 1)
            .read_to_end(&mut data)
            .is_err()
            || data.len() > remaining
        {
            continue;
        }
        let ids = db
            .faces()
            .filter(|face| matches!(&face.source, Source::File(source) if source == &path))
            .map(|face| face.id)
            .collect::<Vec<_>>();
        let previous = db.len();
        let bytes = data.len();
        db.load_font_data(data);
        if db.len() > previous {
            // Replace file-backed faces only after the same bytes parse. Keep
            // all weights in a TTC collection and share one allocation.
            for id in ids {
                db.remove_face(id);
            }
            retained += bytes;
        }
    }
    retained
}

pub(crate) fn options() -> &'static usvg::Options<'static> {
    static OPTIONS: LazyLock<usvg::Options<'static>> = LazyLock::new(|| {
        let mut options = usvg::Options::default();
        let db = options.fontdb_mut();
        db.load_system_fonts();
        retain_font_bytes(db, FONT_BYTES_LIMIT);
        options
    });
    &OPTIONS
}

pub(crate) fn decode(diagram: &Diagram, size: RasterSize) -> Result<Arc<RenderImage>, String> {
    // Both callers already bound their targets; validate before parsing or allocating.
    if size.width == 0
        || size.height == 0
        || size.width > 16384
        || size.height > 16384
        || u64::from(size.width) * u64::from(size.height) > 16_777_216
    {
        return Err("invalid diagram pixel target".into());
    }
    let image = diagram.image_at_size(size);
    let tree =
        usvg::Tree::from_data(image.bytes(), options()).map_err(|error| error.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width, size.height)
        .ok_or("invalid diagram pixel allocation")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    // Match GPUI 0.2.2's SVG branch byte for byte, including premultiplied alpha.
    let buffer = image::RgbaImage::from_raw(size.width, size.height, pixmap.take())
        .ok_or("invalid diagram pixel buffer")?;
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_primary_fonts_are_shared_bytes_with_a_fixed_budget() {
        let mut db = Database::new();
        db.load_system_fonts();
        let query = Query {
            families: &[Family::Name(FONT_FAMILIES[0])],
            ..Default::default()
        };
        let primary = db.query(&query);
        #[cfg(target_os = "windows")]
        assert!(
            primary.is_some(),
            "Windows test needs its system Chinese font"
        );
        if primary.is_none() {
            return;
        }
        let bytes = retain_font_bytes(&mut db, FONT_BYTES_LIMIT);
        assert!(bytes > 0 && bytes <= FONT_BYTES_LIMIT);
        let id = db.query(&query).unwrap();
        assert!(matches!(db.face_source(id), Some((Source::Binary(_), _))));
        // Once promoted, repeated calls must retain no additional copy.
        assert_eq!(retain_font_bytes(&mut db, FONT_BYTES_LIMIT), 0);
        let mut db = Database::new();
        db.load_system_fonts();
        assert_eq!(retain_font_bytes(&mut db, 1), 0);
    }

    #[gpui::test]
    fn pixels_match_gpui_for_text_shapes_transparency_and_both_themes(
        cx: &mut gpui::TestAppContext,
    ) {
        let renderer = cx.update(|cx| cx.svg_renderer());
        let sources = std::env::var_os("TINY_MD_DIAGRAM_DOCUMENT").map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let (blocks, _) = crate::code_blocks::collect(&text.lines().map(str::to_owned).collect::<Vec<_>>());
            blocks.into_iter().filter(|block| block.is_mermaid()).map(|block| block.source).collect::<Vec<_>>()
        }).unwrap_or_else(|| vec!["flowchart LR\nA[开始 ABC ✓ ①] -->|继续| B{确认}\nB --> C((结束))\nstyle A fill:#123456,stroke:#abcdef".into()]);
        let mut compared = 0;
        for dark in [false, true] {
            for source in &sources {
                let Ok(diagram) = crate::diagrams::render(&crate::diagrams::DiagramKey {
                    source: source.clone(),
                    dark,
                }) else {
                    continue;
                };
                let (width, height) = diagram.display_size(1000.0, None);
                let size = RasterSize::for_display(width, height, 2.0);
                let expected = diagram
                    .image_at_size(size)
                    .to_image_data(renderer.clone())
                    .unwrap();
                let actual = decode(&diagram, size).unwrap();
                assert_eq!(actual.size(0), expected.size(0));
                assert!(
                    actual.as_bytes(0) == expected.as_bytes(0),
                    "SVG pixels must remain identical for dark={dark}"
                );
                assert!(
                    actual
                        .as_bytes(0)
                        .unwrap()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|pixel| pixel[3] != 0)
                );
                compared += 1;
            }
        }
        assert!(compared > 0);
        println!(
            "raster_parity: source_blocks={} compared_light_and_dark={compared}",
            sources.len()
        );
    }

    #[test]
    fn out_of_budget_targets_fail_before_allocating_pixels() {
        let diagram = Diagram {
            image: Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Svg, vec![])),
            width: 1.0,
            height: 1.0,
        };
        for size in [
            RasterSize {
                width: 0,
                height: 1,
            },
            RasterSize {
                width: 16385,
                height: 1,
            },
            RasterSize {
                width: 8192,
                height: 8192,
            },
        ] {
            assert_eq!(
                decode(&diagram, size).unwrap_err(),
                "invalid diagram pixel target"
            );
        }
    }
}
