use crate::{
    Asset, CURRENT_VERSION, Installation, MAX_DOWNLOAD, Release, redirect_policy, validate_asset,
    validate_digest,
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct CancelInner {
    flag: AtomicBool,
    notify: tokio::sync::Notify,
}
#[derive(Clone, Default)]
pub struct Cancellation(Arc<CancelInner>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::Release);
        self.0.notify.notify_one();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::Acquire)
    }
    async fn cancelled(&self) {
        if !self.is_cancelled() {
            self.0.notify.notified().await;
        }
    }
}

pub struct PreparedUpdate {
    pub payload: PathBuf,
    pub asset: Asset,
    pub version: String,
    pub kind: Installation,
    pub(crate) archive: PathBuf,
    pub(crate) staging: Mutex<Option<tempfile::TempDir>>,
}

pub fn verify_file(path: &Path, digest: &str, expected_size: u64) -> Result<()> {
    let expected = validate_digest(digest)?;
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    ensure!(
        size > 0 && size <= MAX_DOWNLOAD && (expected_size == 0 || size == expected_size),
        "更新文件大小不匹配"
    );
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    ensure!(
        format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected),
        "更新文件 SHA-256 校验失败"
    );
    Ok(())
}

struct Writer {
    file: File,
    hash: Sha256,
    written: u64,
    expected: u64,
}
impl Writer {
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let size = self.written.saturating_add(bytes.len() as u64);
        ensure!(
            size <= MAX_DOWNLOAD && (self.expected == 0 || size <= self.expected),
            "下载超过声明的大小限制"
        );
        self.file.write_all(bytes)?;
        self.hash.update(bytes);
        self.written = size;
        Ok(())
    }
    fn finish(self, digest: &str) -> Result<()> {
        ensure!(
            self.written > 0 && (self.expected == 0 || self.written == self.expected),
            "下载不完整"
        );
        ensure!(
            format!("{:x}", self.hash.finalize()).eq_ignore_ascii_case(validate_digest(digest)?),
            "下载 SHA-256 校验失败"
        );
        self.file.sync_all()?;
        Ok(())
    }
}

/// Runs only on a worker. Cancellation interrupts both connection and reads.
pub fn download(
    release: &Release,
    kind: Installation,
    cancel: &Cancellation,
    progress: impl FnMut(u64, u64),
) -> Result<PreparedUpdate> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(download_async(release, kind, cancel, progress))
}

async fn download_async(
    release: &Release,
    kind: Installation,
    cancel: &Cancellation,
    mut progress: impl FnMut(u64, u64),
) -> Result<PreparedUpdate> {
    let asset = release.asset(kind)?;
    validate_asset(&release.version, &asset, true)?;
    ensure!(!cancel.is_cancelled(), "下载已取消");
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(redirect_policy())
        .user_agent(format!("tiny-md/{CURRENT_VERSION}"))
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(900))
        .build()?;
    let response = tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("下载已取消"),
        result = client.get(&asset.url).send() => result?,
    };
    let mut response = response.error_for_status()?;
    let total = response.content_length().unwrap_or(asset.size);
    ensure!(
        total <= MAX_DOWNLOAD && (asset.size == 0 || total == asset.size),
        "服务器返回的文件大小与清单不一致"
    );
    let staging = tempfile::Builder::new()
        .prefix("tiny-md-update-")
        .tempdir()?;
    let archive = staging.path().join(&asset.name);
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&archive)?;
    let mut writer = Writer {
        file,
        hash: Sha256::new(),
        written: 0,
        expected: if asset.size > 0 { asset.size } else { total },
    };
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("下载已取消"),
            result = response.chunk() => result?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        writer.push(&chunk)?;
        progress(writer.written, total);
    }
    writer.finish(&asset.digest)?;
    ensure!(!cancel.is_cancelled(), "下载已取消");
    let payload = if kind == Installation::WindowsInstalled {
        archive.clone()
    } else {
        extract(&archive, &staging.path().join("payload"), kind, cancel)?
    };
    Ok(PreparedUpdate {
        payload,
        asset,
        version: release.version.clone(),
        kind,
        archive,
        staging: Mutex::new(Some(staging)),
    })
}

pub(crate) fn extract(
    archive: &Path,
    directory: &Path,
    kind: Installation,
    cancel: &Cancellation,
) -> Result<PathBuf> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    ensure!(zip.len() <= 4096, "更新包条目过多");
    let mut names = std::collections::HashSet::new();
    let mut expanded = 0_u64;
    for index in 0..zip.len() {
        ensure!(!cancel.is_cancelled(), "下载已取消");
        let mut entry = zip.by_index(index)?;
        ensure!(!entry.is_symlink(), "更新包不能包含符号链接");
        let path = entry.enclosed_name().context("更新包包含路径穿越")?;
        let name = path.to_str().context("更新包路径必须为 UTF-8")?;
        ensure!(
            !name.contains(['\\', ':']) && names.insert(name.to_ascii_lowercase()),
            "更新包路径不安全或重复"
        );
        let allowed = match kind {
            Installation::WindowsPortable => {
                ["tiny-md.exe", "version.txt", "welcome.md"].contains(&name)
            }
            Installation::MacApp => name == "Tiny MD.app" || name.starts_with("Tiny MD.app/"),
            Installation::WindowsInstalled => false,
        };
        ensure!(allowed, "更新包布局与应用不一致");
        expanded = expanded
            .checked_add(entry.size())
            .context("更新包展开大小溢出")?;
        ensure!(expanded <= MAX_DOWNLOAD * 2, "更新包展开大小超限");
        let target = directory.join(&path);
        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        std::fs::create_dir_all(target.parent().context("更新路径没有父目录")?)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        let mut count = 0_u64;
        let mut bytes = [0_u8; 64 * 1024];
        loop {
            ensure!(!cancel.is_cancelled(), "下载已取消");
            let read = entry.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            count += read as u64;
            ensure!(count <= entry.size(), "更新包展开大小与声明不符");
            file.write_all(&bytes[..read])?;
        }
        ensure!(count == entry.size(), "更新包条目截断");
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                file.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))?;
            }
        }
    }
    let payload = match kind {
        Installation::WindowsPortable => directory.join("tiny-md.exe"),
        _ => directory.join("Tiny MD.app"),
    };
    let binary = if kind == Installation::MacApp {
        payload.join("Contents/MacOS/tiny-md")
    } else {
        payload.clone()
    };
    ensure!(
        binary.is_file() && std::fs::metadata(&binary)?.len() > 0,
        "更新包缺少应用程序"
    );
    if kind == Installation::MacApp {
        ensure!(
            payload.join("Contents/Info.plist").is_file(),
            "更新包缺少应用信息"
        );
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ABC: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    #[test]
    fn stream_checks_truncation_extra_bytes_and_digest() {
        for (bytes, size, digest, valid) in [
            (b"abc".as_slice(), 3, ABC, true),
            (b"ab", 3, ABC, false),
            (b"abcd", 3, ABC, false),
            (b"xyz", 3, ABC, false),
        ] {
            let file = tempfile::tempfile().unwrap();
            let mut writer = Writer {
                file,
                hash: Sha256::new(),
                written: 0,
                expected: size,
            };
            let result = writer.push(bytes).and_then(|_| writer.finish(digest));
            assert_eq!(result.is_ok(), valid);
        }
    }
    #[test]
    fn archive_rejects_traversal_extra_files_and_cancelled_extraction() {
        for name in [
            "../tiny-md.exe",
            "C:/tiny-md.exe",
            "other.exe",
            "tiny-md.exe",
        ] {
            let root = tempfile::tempdir().unwrap();
            let archive = root.path().join("test.zip");
            let mut zip = zip::ZipWriter::new(File::create(&archive).unwrap());
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"binary").unwrap();
            zip.finish().unwrap();
            let cancel = Cancellation::default();
            if name == "tiny-md.exe" {
                cancel.cancel();
            }
            assert!(
                extract(
                    &archive,
                    &root.path().join("unpack"),
                    Installation::WindowsPortable,
                    &cancel
                )
                .is_err()
            );
            assert!(!root.path().join("tiny-md.exe").exists());
        }
    }
}
