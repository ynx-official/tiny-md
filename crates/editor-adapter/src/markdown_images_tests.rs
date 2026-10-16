use super::*;
use std::io::Write;

fn images(source: &str) -> Vec<Vec<ImageLink>> {
    collect(&source.split('\n').map(str::to_owned).collect::<Vec<_>>())
}

#[test]
fn markdown_image_syntax_preserves_unicode_titles_escapes_and_references() {
    let rows = images(
        "![方案阅读方式对比](../02-design/assets/p5-p6-plan-reading-comparison-v1.png)\n文字 ![甲](<图 片.png> \"标题\") ![乙](https://example.test/a(b).png)\n![引用][PIC]\n\n[pic]: ./a%20b.png",
    );
    assert!(rows[0][0].standalone);
    assert_eq!(
        rows[0][0].target,
        "../02-design/assets/p5-p6-plan-reading-comparison-v1.png"
    );
    assert_eq!(rows[0][0].alt, "方案阅读方式对比");
    assert_eq!(rows[1].len(), 2);
    assert!(!rows[1][0].standalone);
    assert_eq!(rows[1][0].target, "图 片.png");
    assert_eq!(rows[1][1].target, "https://example.test/a(b).png");
    assert_eq!(rows[2][0].target, "./a%20b.png");
    assert_eq!(images(r"![图](a\(b\).png)")[0][0].target, "a(b).png");
}

#[test]
fn markdown_image_examples_code_and_frontmatter_do_not_load() {
    for text in [
        r"![图]\(a.png)",
        r"\![图](a.png)",
        "`![图](a.png)`",
        "```md\n![图](a.png)\n```",
        "    ![图](a.png)",
        "---\nimage: ![图](a.png)\n---",
        "![missing][unknown]",
    ] {
        assert!(images(text).iter().all(Vec::is_empty), "{text}");
    }
}

#[test]
fn markdown_image_paths_use_document_directory_and_decode_once() {
    let folder = tempfile::tempdir().unwrap();
    let document = folder.path().join("需求").join("计划.md");
    assert_eq!(
        resolve("../设计/图%20片%2520.png", Some(&document)).unwrap(),
        Target::Local(document.parent().unwrap().join("../设计/图 片%20.png"))
    );
    assert!(resolve("a.png", None).unwrap_err().contains("先保存"));
    let path = folder.path().join("图片.png");
    assert_eq!(
        resolve(path.to_str().unwrap(), None).unwrap(),
        Target::Local(path.clone())
    );
    assert_eq!(
        resolve(Url::from_file_path(&path).unwrap().as_str(), None).unwrap(),
        Target::Local(path)
    );
    assert!(matches!(
        resolve("https://example.test/图.png?v=1#x", None).unwrap(),
        Target::Remote(_)
    ));
    for source in [
        "",
        "javascript:alert(1)",
        "data:image/png;base64,abc",
        "ftp://example.test/a.png",
        "https://name:secret@example.test/a.png",
    ] {
        assert!(resolve(source, None).is_err());
    }
}

#[cfg(windows)]
#[test]
fn markdown_image_windows_drive_and_unc_are_local() {
    for path in [
        r"C:\笔记\图.png",
        "C:/notes/image.png",
        r"\\server\share\image.png",
    ] {
        assert_eq!(
            resolve(path, None).unwrap(),
            Target::Local(PathBuf::from(path))
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn markdown_image_macos_absolute_path_is_local() {
    assert_eq!(
        resolve("/Users/writer/笔记/图.png", None).unwrap(),
        Target::Local(PathBuf::from("/Users/writer/笔记/图.png"))
    );
}

pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba([255, 0, 0, 255]),
    ))
    .write_to(&mut output, image::ImageFormat::Png)
    .unwrap();
    output.into_inner()
}

#[test]
fn markdown_image_decode_load_and_sizing_are_bounded_and_bgra() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("红 色.png");
    std::fs::write(&path, png(400, 200)).unwrap();
    let image = load(&Target::Local(path)).unwrap();
    assert_eq!(image.as_bytes(0).unwrap()[..4], [0, 0, 255, 255]);
    assert_eq!(display_size(&image, 200.0), (200.0, 100.0));
    assert_eq!(display_size(&image, 800.0), (400.0, 200.0));
    assert!(decode(b"not an image").is_err());
    assert!(check_size(16384, 16384).is_err());
    assert!(check_size(1, 16385).is_err());
    assert!(read_limited(std::io::repeat(0).take(MAX_BYTES + 1)).is_err());
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"##;
    let image = decode(svg).unwrap();
    assert_eq!(display_size(&image, 100.0), (20.0, 10.0));
    assert_eq!(image.as_bytes(0).unwrap()[..4], [0, 0, 255, 255]);
}

#[test]
fn markdown_image_http_download_redirect_and_error_use_real_transport() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        for (status, headers, body) in [
            ("302 Found", "Location: /image.png\r\n", Vec::new()),
            ("200 OK", "Content-Type: image/png\r\n", png(4, 2)),
            ("404 Not Found", "", Vec::new()),
            ("200 OK", "", b"not a picture".to_vec()),
        ] {
            let (mut socket, _) = server.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 16384, "unexpectedly large test request");
            }
            write!(
                socket,
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            socket.write_all(&body).unwrap();
        }
    });
    assert_eq!(
        display_size(
            &load(&Target::Remote(format!("{base}/redirect"))).unwrap(),
            100.0
        ),
        (4.0, 2.0)
    );
    assert!(
        load(&Target::Remote(format!("{base}/missing")))
            .err()
            .unwrap()
            .contains("404")
    );
    assert!(load(&Target::Remote(format!("{base}/invalid"))).is_err());
    worker.join().unwrap();
}
