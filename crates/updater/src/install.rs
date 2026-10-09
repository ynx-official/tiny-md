//! A copied application binary acts as the local helper. It starts only after
//! every writing window has approved exit; a cancelled save creates no helper.
use crate::{Cancellation, Installation, PreparedUpdate, verify_file, version};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
struct Plan {
    process: u32,
    current: PathBuf,
    archive: PathBuf,
    digest: String,
    size: u64,
    version: String,
    kind: String,
}

pub fn installation() -> Result<Installation> {
    #[cfg(target_os = "windows")]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let current = std::env::current_exe()?.canonicalize()?;
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{84E4C0ED-D995-4CA1-B15C-E75A49FDC657}_is1");
        if let Ok(key) = key
            && let Ok(location) = key.get_value::<String, _>("InstallLocation")
            && let Ok(location) = PathBuf::from(location).canonicalize()
            && current.parent().is_some_and(|parent| {
                parent
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&location.to_string_lossy())
            })
        {
            return Ok(Installation::WindowsInstalled);
        }
        Ok(Installation::WindowsPortable)
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(Installation::MacApp)
    }
    #[cfg(not(any(
        target_os = "windows",
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    {
        bail!("该平台尚无在线更新产物")
    }
}

fn command(path: impl AsRef<std::ffi::OsStr>) -> Command {
    let command = Command::new(path);
    #[cfg(windows)]
    let command = {
        let mut command = command;
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        command
    };
    command
}

fn binary_version(binary: &Path, expected: &str) -> Result<()> {
    let result = command(binary)
        .arg("--version")
        .output()
        .context("无法检查更新程序的版本")?;
    ensure!(
        result.status.success()
            && String::from_utf8_lossy(&result.stdout).trim()
                == format!("Tiny MD {}", version(expected)?),
        "更新程序的实际版本与发布标签不一致"
    );
    Ok(())
}

#[cfg(windows)]
fn installer_path(path: &Path) -> Result<PathBuf> {
    use std::{
        ffi::OsString,
        os::windows::ffi::{OsStrExt, OsStringExt},
        path::{Component, Prefix},
    };
    ensure!(path.is_absolute(), "安装器路径必须为绝对路径");
    // Rust canonicalize returns extended-length paths. Inno Setup 6 treats
    // their '?' as an invalid directory character; normalize only disk/UNC
    // prefixes at this boundary, preserving every UTF-16 filename character.
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) | Prefix::UNC(_, _) => Ok(path.to_owned()),
            Prefix::VerbatimDisk(_) => {
                let wide: Vec<_> = path.as_os_str().encode_wide().skip(4).collect();
                Ok(PathBuf::from(OsString::from_wide(&wide)))
            }
            Prefix::VerbatimUNC(_, _) => {
                let wide: Vec<_> = [b'\\' as u16, b'\\' as u16]
                    .into_iter()
                    .chain(path.as_os_str().encode_wide().skip(8))
                    .collect();
                Ok(PathBuf::from(OsString::from_wide(&wide)))
            }
            _ => bail!("安装器不支持设备路径"),
        },
        _ => bail!("安装器路径缺少磁盘或共享目录"),
    }
}

#[cfg(windows)]
fn installer_command(payload: &Path, parent: &Path, log: &Path) -> Result<Command> {
    let mut setup = command(payload);
    setup.args([
        "/SILENT",
        "/SUPPRESSMSGBOXES",
        "/SP-",
        "/NORESTART",
        "/CURRENTUSER",
        "/NOCLOSEAPPLICATIONS",
    ]);
    for (key, path) in [("/DIR=", parent), ("/LOG=", log)] {
        let mut argument = std::ffi::OsString::from(key);
        argument.push(installer_path(path)?);
        setup.arg(argument);
    }
    Ok(setup)
}

/// Report failures after handoff without initializing another GPUI editor.
/// Before the handshake, the original application's update dialog reports them.
pub fn show_helper_failure(plan_path: &Path, error: &str) {
    #[cfg(windows)]
    if let Some(root) = plan_path.parent()
        && root.join("helper-ready").is_file()
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                flags: u32,
            ) -> i32;
        }
        let message = format!(
            "Tiny MD 更新未完成。\n\n{error}\n\n诊断日志：{}",
            root.join("install.log").display()
        );
        let wide = |text: &str| {
            std::ffi::OsStr::new(text)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let message = wide(&message);
        let caption = wide("Tiny MD 更新失败");
        // Owned, NUL-terminated buffers remain alive for the modal native dialog.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                caption.as_ptr(),
                0x10 | 0x10000,
            );
        }
    }
    #[cfg(not(windows))]
    let _ = (plan_path, error);
}

/// A successful return means an acknowledged helper is waiting for this process.
/// The UI must then quit; on error it must keep the writing windows open.
pub fn launch(prepared: &PreparedUpdate) -> Result<()> {
    ensure!(
        !cfg!(debug_assertions),
        "开发构建不能安装更新，请使用发行包"
    );
    verify_file(
        &prepared.archive,
        &prepared.asset.digest,
        prepared.asset.size,
    )?;
    let guard = prepared
        .staging
        .lock()
        .map_err(|_| anyhow::anyhow!("更新暂存状态不可用"))?;
    let root = guard.as_ref().context("更新已经交接")?.path().to_path_buf();
    let current = std::env::current_exe()?.canonicalize()?;
    let plan = Plan {
        process: std::process::id(),
        current: current.clone(),
        archive: prepared.archive.clone(),
        digest: prepared.asset.digest.clone(),
        size: prepared.asset.size,
        version: prepared.version.clone(),
        kind: match prepared.kind {
            Installation::WindowsInstalled => "setup",
            Installation::WindowsPortable => "portable",
            Installation::MacApp => "mac",
        }
        .into(),
    };
    #[cfg(windows)]
    let helper_name = "tiny-md-update-helper.exe";
    #[cfg(not(windows))]
    let helper_name = "tiny-md-update-helper";
    let helper = root.join(helper_name);
    fs::copy(&current, &helper)?;
    let plan_path = root.join("install-plan.json");
    let mut file = File::create(&plan_path)?;
    serde_json::to_writer(&mut file, &plan)?;
    file.sync_all()?;
    let log = File::create(root.join("install.log"))?;
    let mut child = command(&helper)
        .arg("--apply-update")
        .arg(&plan_path)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
        .context("无法启动更新助手")?;
    // Handshake catches unsupported helper builds and plan validation errors
    // before the UI discards its windows. Timeout cancels the waiting child.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("helper-ready").is_file() {
        if child.try_wait()?.is_some() {
            bail!(
                "更新助手启动失败，请查看 {}",
                root.join("install.log").display()
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("更新助手未就绪，文档窗口已保留");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(guard);
    if let Some(dir) = prepared
        .staging
        .lock()
        .map_err(|_| anyhow::anyhow!("更新暂存状态不可用"))?
        .take()
    {
        let _ = dir.keep();
    }
    Ok(())
}

/// Called before GPUI initializes. Helper errors never start a blank editor.
pub fn helper_main(plan_path: &Path) -> Result<()> {
    let root = plan_path
        .parent()
        .context("更新计划缺少目录")?
        .canonicalize()?;
    ensure!(
        root.starts_with(std::env::temp_dir().canonicalize()?)
            && root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("tiny-md-update-")),
        "助手目录必须为应用专属临时目录"
    );
    let bytes = fs::read(plan_path)?;
    ensure!(bytes.len() < 16 * 1024, "更新计划过大");
    let plan: Plan = serde_json::from_slice(&bytes)?;
    ensure!(
        plan.process > 0 && plan.process != std::process::id(),
        "更新进程 ID 无效"
    );
    ensure!(
        plan.current.is_absolute() && plan.current.is_file(),
        "原程序路径无效"
    );
    ensure!(
        plan.archive.canonicalize()?.parent() == Some(root.as_path()),
        "更新包必须位于助手目录内"
    );
    let helper = std::env::current_exe()?.canonicalize()?;
    ensure!(
        helper.parent() == Some(root.as_path()) && helper != plan.current,
        "助手不能从原安装位置执行"
    );
    version(&plan.version)?;
    verify_file(&plan.archive, &plan.digest, plan.size)?;
    let kind = match plan.kind.as_str() {
        "setup" => Installation::WindowsInstalled,
        "portable" => Installation::WindowsPortable,
        "mac" => Installation::MacApp,
        _ => bail!("未知更新安装类型"),
    };
    let fresh = tempfile::Builder::new()
        .prefix("verified-")
        .tempdir_in(&root)?;
    let payload = if kind == Installation::WindowsInstalled {
        plan.archive.clone()
    } else {
        crate::download::extract(&plan.archive, fresh.path(), kind, &Cancellation::default())?
    };
    if kind != Installation::WindowsInstalled {
        let binary = if kind == Installation::MacApp {
            payload.join("Contents/MacOS/tiny-md")
        } else {
            payload.clone()
        };
        binary_version(&binary, &plan.version)?;
    }
    #[cfg(target_os = "macos")]
    validate_mac_bundle(&plan, &payload)?;
    #[cfg(windows)]
    {
        if plan.kind == "setup" {
            installer_path(plan.current.parent().context("安装目录无效")?)?;
            installer_path(&root.join("installer.log"))?;
        }
        ensure!(
            !fs::metadata(&plan.current)?.permissions().readonly(),
            "安装程序文件为只读，未退出写作窗口"
        );
        let _probe =
            tempfile::NamedTempFile::new_in(plan.current.parent().context("安装目录无效")?)?;
    }
    fs::write(root.join("helper-ready"), b"ready")?;
    wait_for_parent(plan.process)?;
    let result = apply(&plan, &payload);
    if let Err(e) = &result {
        eprintln!(
            "更新失败：{e:#}。诊断日志：{}",
            root.join("install.log").display()
        );
    }
    // Failed updates retain their evidence and payload for diagnosis. Successful
    // ones remove only fixed files in their own exclusive temporary directory.
    if result.is_ok() {
        drop(fresh);
        #[cfg(windows)]
        {
            let cleanup = root.join("cleanup.ps1");
            fs::write(&cleanup, include_str!("cleanup-windows.ps1"))?;
            command("powershell.exe")
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(cleanup)
                .arg("-Root")
                .arg(&root)
                .arg("-ParentProcess")
                .arg(std::process::id().to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        #[cfg(not(windows))]
        fs::remove_dir_all(root)?;
    }
    result
}

fn wait_for_parent(process: u32) -> Result<()> {
    #[cfg(windows)]
    {
        let status = command("powershell.exe").args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!("Wait-Process -Id {process} -Timeout 120 -ErrorAction SilentlyContinue; if (Get-Process -Id {process} -ErrorAction SilentlyContinue) {{ exit 1 }}"))
            .status()?;
        ensure!(status.success(), "等待旧应用退出超时，未执行安装");
    }
    #[cfg(not(windows))]
    {
        let deadline = Instant::now() + Duration::from_secs(120);
        while command("/bin/kill")
            .args(["-0", &process.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
        {
            ensure!(Instant::now() < deadline, "等待旧应用退出超时，未执行安装");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}

fn apply(plan: &Plan, payload: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        // Inno's default CloseApplications can terminate another instance with
        // unsaved writing. Only inspect processes using this installation path.
        let guard_script = plan
            .archive
            .parent()
            .context("更新包目录无效")?
            .join("check-other-instances.ps1");
        fs::write(&guard_script, include_str!("check-running-windows.ps1"))?;
        let status = command("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&guard_script)
            .arg("-Executable")
            .arg(&plan.current)
            .status()?;
        ensure!(status.success(), "仍有其他 Tiny MD 进程，请关闭后重试");
        let parent = plan.current.parent().context("安装目录无效")?;
        let result = with_backup(&plan.current, || {
            if plan.kind == "setup" {
                let log = guard_script.with_file_name("installer.log");
                let status = installer_command(payload, parent, &log)?.status()?;
                ensure!(
                    status.success(),
                    "安装程序失败：{status}。安装日志：{}",
                    log.display()
                );
            } else {
                let mut staged = tempfile::NamedTempFile::new_in(parent)?;
                std::io::copy(&mut File::open(payload)?, &mut staged)?;
                staged.as_file().sync_all()?;
                staged.persist(&plan.current).map_err(|e| e.error)?;
            }
            binary_version(&plan.current, &plan.version)?;
            command(&plan.current)
                .current_dir(parent)
                .spawn()
                .context("新版应用启动失败")?;
            Ok(())
        });
        if result.is_err() {
            let _ = command(&plan.current).current_dir(parent).spawn();
        }
        result
    }
    #[cfg(target_os = "macos")]
    {
        let target = plan
            .current
            .ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .context("请先将 Tiny MD.app 安装到可写目录")?;
        ensure!(
            !target.starts_with("/Volumes"),
            "不能更新 DMG 中的应用，请先安装到 Applications"
        );
        let parent = target.parent().context("应用目录无效")?;
        let plist = payload.join("Contents/Info.plist");
        let id = command("/usr/bin/plutil")
            .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
            .arg(&plist)
            .output()?;
        ensure!(
            id.status.success() && String::from_utf8_lossy(&id.stdout).trim() == "dev.tiny-md.app",
            "更新包应用标识不一致"
        );
        ensure!(
            command("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(payload)
                .status()?
                .success(),
            "更新应用签名校验失败"
        );
        let swap = tempfile::Builder::new()
            .prefix(".tiny-md-swap-")
            .tempdir_in(parent)?;
        let staged = swap.path().join("new.app");
        let backup = swap.path().join("old.app");
        ensure!(
            command("/usr/bin/ditto")
                .arg(payload)
                .arg(&staged)
                .status()?
                .success(),
            "无法暂存完整应用"
        );
        fs::rename(target, &backup)?;
        let result = (|| {
            fs::rename(&staged, target)?;
            ensure!(
                command("/usr/bin/open").arg(target).status()?.success(),
                "新版应用启动失败"
            );
            Ok(())
        })();
        if result.is_err() {
            if target.exists() {
                fs::rename(target, &staged)?;
            }
            if let Err(error) = fs::rename(&backup, target) {
                let path = swap.keep();
                bail!("无法恢复旧应用：{error}，备份位于 {}", path.display());
            }
            let _ = command("/usr/bin/open").arg(target).status();
        }
        result
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (plan, payload);
        bail!("该平台尚未实现更新安装")
    }
}

#[cfg(target_os = "macos")]
fn validate_mac_bundle(plan: &Plan, payload: &Path) -> Result<()> {
    ensure!(plan.kind == "mac", "macOS 不支持该更新安装类型");
    let target = plan
        .current
        .ancestors()
        .find(|path| path.extension().is_some_and(|e| e == "app"))
        .context("请先安装 Tiny MD.app")?;
    ensure!(
        !target.starts_with("/Volumes"),
        "不能更新 DMG 中的应用，请先安装到 Applications"
    );
    let parent = target.parent().context("应用目录无效")?;
    // Preflight write permission before telling the UI it is safe to quit.
    let _probe = tempfile::Builder::new()
        .prefix(".tiny-md-write-check-")
        .tempdir_in(parent)?;
    let plist = payload.join("Contents/Info.plist");
    for (key, expected) in [
        ("CFBundleIdentifier", "dev.tiny-md.app".to_owned()),
        ("CFBundleShortVersionString", {
            let v = version(&plan.version)?;
            format!("{}.{}.{}", v.major, v.minor, v.patch)
        }),
    ] {
        let info = command("/usr/bin/plutil")
            .args(["-extract", key, "raw", "-o", "-"])
            .arg(&plist)
            .output()?;
        ensure!(
            info.status.success() && String::from_utf8_lossy(&info.stdout).trim() == expected,
            "更新包 {key} 不一致"
        );
    }
    ensure!(
        command("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(payload)
            .status()?
            .success(),
        "更新应用签名校验失败"
    );
    Ok(())
}

#[cfg(windows)]
fn with_backup(current: &Path, operation: impl FnOnce() -> Result<()>) -> Result<()> {
    let parent = current.parent().context("安装目录无效")?;
    let backup = tempfile::Builder::new()
        .prefix(".tiny-md-backup-")
        .tempdir_in(parent)?;
    let mut files = vec![current.to_owned()];
    // Inno also replaces these bundle files. Restore them with the executable
    // so a failed installer cannot leave mismatched version metadata.
    for name in ["version.txt", "welcome.md"] {
        let path = parent.join(name);
        if path.is_file() {
            files.push(path);
        }
    }
    for path in &files {
        fs::copy(
            path,
            backup.path().join(path.file_name().context("文件名无效")?),
        )?;
    }
    let result = operation();
    if result.is_err() {
        for path in &files {
            if let Err(error) = fs::copy(
                backup.path().join(path.file_name().context("文件名无效")?),
                path,
            ) {
                let retained = backup.keep();
                bail!(
                    "更新失败且无法恢复原程序：{error}。备份位于 {}",
                    retained.display()
                );
            }
        }
    }
    result
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn installer_receives_normal_drive_and_unc_paths_without_losing_special_characters() {
        for (source, expected) in [
            (
                r"\\?\C:\Users\中文 app's $() & folder",
                r"C:\Users\中文 app's $() & folder",
            ),
            (r"\\?\UNC\server\share\Tiny MD", r"\\server\share\Tiny MD"),
            (r"C:\Tiny MD", r"C:\Tiny MD"),
            (r"\\server\share\Tiny MD", r"\\server\share\Tiny MD"),
        ] {
            assert_eq!(
                installer_path(Path::new(source)).unwrap(),
                Path::new(expected)
            );
        }
        for path in [
            r"relative\Tiny MD",
            r"C:Tiny MD",
            r"\\.\PhysicalDrive0",
            r"\\?\Volume{123}\Tiny MD",
        ] {
            assert!(installer_path(Path::new(path)).is_err(), "accepted {path}");
        }
    }

    #[test]
    fn installed_update_shows_progress_and_records_a_log_without_closing_other_apps() {
        let setup = installer_command(
            Path::new(r"C:\temp\setup.exe"),
            Path::new(r"\\?\C:\安装目录 app's $() & folder"),
            Path::new(r"\\?\C:\temp\installer.log"),
        )
        .unwrap();
        let args: Vec<_> = setup
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "/SILENT"));
        assert!(!args.iter().any(|arg| arg == "/VERYSILENT"));
        assert!(args.iter().any(|arg| arg == "/NOCLOSEAPPLICATIONS"));
        assert!(
            args.iter()
                .any(|arg| arg == r"/DIR=C:\安装目录 app's $() & folder")
        );
        assert!(args.iter().any(|arg| arg == r"/LOG=C:\temp\installer.log"));
    }
    #[test]
    fn failed_replacement_restores_executable_and_version_in_paths_with_special_characters() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("中文 app's $() & folder");
        fs::create_dir(&parent).unwrap();
        let current = parent.join("tiny-md.exe");
        fs::write(&current, b"old binary").unwrap();
        fs::write(parent.join("version.txt"), b"old version").unwrap();
        let result = with_backup(&current, || {
            fs::write(&current, b"new binary")?;
            fs::write(parent.join("version.txt"), b"new version")?;
            bail!("simulated launch failure")
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&current).unwrap(), b"old binary");
        assert_eq!(
            fs::read(parent.join("version.txt")).unwrap(),
            b"old version"
        );
    }
    #[test]
    fn successful_replacement_keeps_new_bytes_and_cleans_backup() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("tiny-md.exe");
        fs::write(&current, b"old").unwrap();
        with_backup(&current, || {
            fs::write(&current, b"new")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"new");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
