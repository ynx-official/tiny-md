//! Markdown image discovery and bounded, platform-neutral resource loading.
use gpui::RenderImage;
use markdown::mdast::Node;
use reqwest::Url;
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};

pub(crate) const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_777_216;
pub(crate) const GAP: f32 = 12.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageLink {
    pub target: String,
    pub alt: String,
    pub standalone: bool,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub align: u8,
    pub source_offset: usize,
    pub href: Option<String>,
}

#[cfg(test)]
pub(crate) fn collect(lines: &[String]) -> Vec<Vec<ImageLink>> {
    let source = lines.join("\n");
    let root = crate::html::parse_root(&source);
    let html = crate::html::Document::parse(&source, root.as_ref());
    collect_from_root(lines, &source, root.as_ref(), &html)
}

pub(crate) fn collect_from_root(
    lines: &[String],
    source: &str,
    root: Option<&Node>,
    html: &crate::html::Document,
) -> Vec<Vec<ImageLink>> {
    let mut rows = vec![Vec::new(); lines.len()];
    // Most edits contain no images. Avoid a second Markdown parse in that case.
    let Some(root) = root else {
        return rows;
    };
    fn visit<'a>(node: &'a Node, nodes: &mut Vec<&'a Node>) {
        nodes.push(node);
        if let Some(children) = node.children() {
            for child in children {
                visit(child, nodes);
            }
        }
    }
    let mut nodes = Vec::new();
    visit(root, &mut nodes);
    let mut definitions = HashMap::new();
    for node in &nodes {
        if let Node::Definition(def) = node {
            definitions
                .entry(def.identifier.to_uppercase())
                .or_insert(&def.url);
        }
    }
    for node in nodes {
        let (target, alt) = match node {
            Node::Image(image) => (&image.url, &image.alt),
            Node::ImageReference(image) => {
                let Some(target) = definitions.get(&image.identifier.to_uppercase()) else {
                    continue;
                };
                (*target, &image.alt)
            }
            _ => continue,
        };
        let Some(position) = node.position() else {
            continue;
        };
        let line = position.start.line - 1;
        if let Some(row) = rows.get_mut(line) {
            row.push(ImageLink {
                target: target.clone(),
                alt: alt.clone(),
                standalone: position.start.line == position.end.line
                    && source
                        .get(position.start.offset..position.end.offset)
                        .is_some_and(|image| image == lines[line].trim()),
                width: None,
                height: None,
                align: 0,
                source_offset: position.start.offset,
                href: None,
            });
        }
    }
    for tag in html.image_tags() {
        let line = html.line(tag.range.start);
        let dimension = |name: &str| {
            let css = tag.attrs.get("style").and_then(|s| {
                s.split(';')
                    .filter_map(|d| d.split_once(':'))
                    .filter(|(key, _)| key.trim().eq_ignore_ascii_case(name))
                    .map(|(_, value)| value.trim())
                    .next_back()
            });
            css.or_else(|| tag.attrs.get(name).map(String::as_str))
                .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .map(|v| v.min(16384.0))
        };
        if let Some(target) = tag.attrs.get("src") {
            rows[line].push(ImageLink {
                target: target.clone(),
                alt: tag.attrs.get("alt").cloned().unwrap_or_default(),
                standalone: source
                    .get(tag.range.clone())
                    .is_some_and(|s| s == lines[line].trim()),
                width: dimension("width"),
                height: dimension("height"),
                align: match tag.attrs.get("align").map(String::as_str) {
                    Some("center") => 1,
                    Some("right") => 2,
                    _ => html.alignment_at(tag.range.start),
                },
                source_offset: tag.range.start,
                href: html.link_at(tag.range.start),
            });
        }
    }
    for row in &mut rows {
        row.sort_by_key(|image| image.source_offset);
    }
    rows
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    Local(PathBuf),
    Remote(String),
}

pub(crate) fn resolve(source: &str, document: Option<&Path>) -> Result<Target, String> {
    if source.is_empty() {
        return Err("图片地址为空".into());
    }
    // Drive letters are paths, not URL schemes. PathBuf handles native absolute
    // and UNC paths; normal Markdown uses '/', which also works on Windows.
    let windows_drive =
        source.as_bytes().get(1) == Some(&b':') && source.as_bytes()[0].is_ascii_alphabetic();
    if !windows_drive && let Ok(url) = Url::parse(source) {
        return match url.scheme() {
            "http" | "https"
                if url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none() =>
            {
                Ok(Target::Remote(url.to_string()))
            }
            "file" => url
                .to_file_path()
                .map(Target::Local)
                .map_err(|_| "无法识别本地文件 URL".into()),
            _ => Err("仅支持本地文件和 HTTP/HTTPS 图片地址".into()),
        };
    }
    let decoded = percent_encoding::percent_decode_str(source)
        .decode_utf8()
        .map_err(|_| "图片路径编码无效")?;
    let path = PathBuf::from(decoded.as_ref());
    if path.is_absolute() {
        return Ok(Target::Local(path));
    }
    let parent = document
        .and_then(Path::parent)
        .ok_or("请先保存 Markdown 文档，再加载相对路径图片")?;
    Ok(Target::Local(parent.join(path)))
}

pub(crate) fn read_limited(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "读取图片失败")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("图片超过 32 MiB 限制".into());
    }
    Ok(bytes)
}

pub(crate) fn load(target: &Target) -> Result<Arc<RenderImage>, String> {
    let bytes = match target {
        Target::Local(path) => {
            read_limited(std::fs::File::open(path).map_err(|_| "找不到图片或没有读取权限")?)?
        }
        Target::Remote(url) => {
            static CLIENT: LazyLock<Result<reqwest::blocking::Client, reqwest::Error>> =
                LazyLock::new(|| {
                    reqwest::blocking::Client::builder()
                        .connect_timeout(Duration::from_secs(10))
                        .timeout(Duration::from_secs(20))
                        .redirect(reqwest::redirect::Policy::limited(5))
                        .build()
                });
            let response = CLIENT
                .as_ref()
                .map_err(|_| "无法初始化图片下载")?
                .get(url)
                .send()
                .map_err(|error| {
                    if error.is_timeout() {
                        "图片下载超时"
                    } else {
                        "图片下载失败，请检查网络"
                    }
                })?;
            if !response.status().is_success() {
                return Err(format!(
                    "图片下载失败（HTTP {}）",
                    response.status().as_u16()
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_BYTES)
            {
                return Err("图片超过 32 MiB 限制".into());
            }
            read_limited(response)?
        }
    };
    decode(&bytes)
}

fn check_size(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        Err("图片尺寸过大（最多 1677 万像素，单边最多 16384 像素）".into())
    } else {
        Ok(())
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Arc<RenderImage>, String> {
    use image::ImageDecoder;
    let mut buffer = if let Ok(format) = image::guess_format(bytes) {
        let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        limits.max_alloc = Some(MAX_PIXELS * 8);
        reader.limits(limits);
        let mut decoder = reader
            .into_decoder()
            .map_err(|_| "图片损坏、尺寸过大或格式不支持")?;
        let (width, height) = decoder.dimensions();
        check_size(width, height)?;
        let orientation = decoder
            .orientation()
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|_| "图片解码失败")?;
        decoded.apply_orientation(orientation);
        decoded.into_rgba8()
    } else {
        return decode_svg(bytes, &svg_options());
    };
    for pixel in buffer.as_chunks_mut::<4>().0.iter_mut() {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

fn svg_options() -> usvg::Options<'static> {
    let mut options = usvg::Options {
        fontdb: crate::svg_fonts::system_fonts(),
        ..Default::default()
    };
    // A remote SVG must never read additional local files via <image href>.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.image_href_resolver.resolve_data = Box::new(|_, _, _| None);
    options
}

fn decode_svg(bytes: &[u8], options: &usvg::Options<'_>) -> Result<Arc<RenderImage>, String> {
    let tree = usvg::Tree::from_data(bytes, options).map_err(|_| "图片损坏或格式不支持")?;
    let size = tree.size().to_int_size();
    check_size(size.width(), size.height())?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size.width(), size.height()).ok_or("图片尺寸无效")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let mut buffer = image::RgbaImage::from_raw(size.width(), size.height(), pixmap.take())
        .ok_or("图片解码失败")?;
    // RESVG emits premultiplied RGBA; GPUI expects straight BGRA.
    for pixel in buffer.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = pixel[3] as f32 / 255.0;
        if alpha > 0.0 {
            for channel in &mut pixel[..3] {
                *channel = (*channel as f32 / alpha).round() as u8;
            }
        }
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

pub(crate) fn display_size(image: &RenderImage, available: f32) -> (f32, f32) {
    let size = image.size(0);
    let width = size.width.0 as f32;
    let height = size.height.0 as f32;
    let scale = (available.max(1.0) / width).min(1.0);
    (width * scale, height * scale)
}

pub(crate) fn display_size_for(
    image: &RenderImage,
    available: f32,
    link: &ImageLink,
) -> (f32, f32) {
    let (natural_w, natural_h) = display_size(image, f32::MAX);
    let (w, h) = match (link.width, link.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, natural_h * w / natural_w),
        (None, Some(h)) => (natural_w * h / natural_h, h),
        _ => (natural_w, natural_h),
    };
    let scale = (available.max(1.0) / w).min(1.0);
    (w * scale, h * scale)
}

#[cfg(test)]
#[path = "markdown_images_tests.rs"]
mod tests;
