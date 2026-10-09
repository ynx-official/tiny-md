use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckMode {
    #[default]
    Startup,
    Interval,
    Disabled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub mode: CheckMode,
    pub interval_hours: u32,
    pub last_checked: u64,
    pub auto_download: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            mode: CheckMode::Startup,
            interval_hours: 24,
            last_checked: 0,
            auto_download: false,
        }
    }
}
impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut prefs: Self = serde_json::from_slice(&bytes).context("更新设置格式错误")?;
                prefs.interval_hours = prefs.interval_hours.clamp(1, 8760);
                Ok(prefs)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("更新设置没有父目录")?;
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
    pub fn delay(&self, now: u64) -> Duration {
        Duration::from_secs(
            (u64::from(self.interval_hours.clamp(1, 8760)) * 3600)
                .saturating_sub(now.saturating_sub(self.last_checked)),
        )
    }
}

pub fn preferences_path() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("找不到 LOCALAPPDATA")?)
        .join("Tiny MD");
    #[cfg(not(target_os = "windows"))]
    let root = PathBuf::from(std::env::var_os("HOME").context("找不到用户目录")?)
        .join("Library/Application Support/Tiny MD");
    Ok(root.join("update-settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_persist_mode_and_clamp_interval() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        assert_eq!(Preferences::load(&path).unwrap().mode, CheckMode::Startup);
        let prefs = Preferences {
            mode: CheckMode::Disabled,
            interval_hours: 0,
            last_checked: 100,
            ..Default::default()
        };
        prefs.save(&path).unwrap();
        let loaded = Preferences::load(&path).unwrap();
        assert_eq!(loaded.mode, CheckMode::Disabled);
        assert_eq!(loaded.interval_hours, 1);
        assert_eq!(loaded.delay(99), Duration::from_secs(3600));
        assert_eq!(loaded.delay(3701), Duration::ZERO);
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Preferences::load(&path).is_err());
    }

    #[test]
    fn background_download_is_opt_in_and_legacy_settings_keep_their_schedule() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("settings.json");
        std::fs::write(
            &path,
            br#"{"mode":"interval","interval_hours":48,"last_checked":123}"#,
        )
        .unwrap();
        let legacy = Preferences::load(&path).unwrap();
        assert_eq!(legacy.interval_hours, 48);
        assert_eq!(legacy.last_checked, 123);
        assert_eq!(
            serde_json::to_value(&legacy).unwrap()["auto_download"].as_bool(),
            Some(false)
        );
        std::fs::write(&path, br#"{"mode":"startup","auto_download":true}"#).unwrap();
        Preferences::load(&path).unwrap().save(&path).unwrap();
        let restored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(restored["auto_download"].as_bool(), Some(true));
    }
}
