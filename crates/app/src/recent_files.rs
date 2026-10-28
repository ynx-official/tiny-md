//! Local recent-file history. Only successful file operations change the list.
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(super) const LIMIT: usize = 20;

#[derive(Default)]
pub(super) struct RecentFiles {
    pub paths: Vec<PathBuf>,
    pub error: Option<String>,
    path: Option<PathBuf>,
    // A malformed file is evidence worth keeping; only explicit Clear replaces it.
    unreadable: bool,
}

#[derive(Deserialize, Serialize)]
struct StoredHistory {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn load() -> Self {
        match tiny_md_updater::preferences_path() {
            Ok(path) => Self::load_at(path.with_file_name("recent-files.json")),
            Err(error) => Self {
                error: Some(format!("无法读取最近文件：{error:#}")),
                unreadable: true,
                ..Self::default()
            },
        }
    }

    pub fn load_at(path: PathBuf) -> Self {
        let result = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<StoredHistory>(&bytes)
                .map(|stored| stored.paths)
                .context("最近文件记录格式错误"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        };
        let mut history = Self {
            path: Some(path),
            ..Self::default()
        };
        match result {
            Ok(paths) => {
                // Do not probe disk/network paths during startup: a removable drive
                // can be offline without the user wanting to forget its documents.
                for path in paths {
                    if path.is_absolute() && !history.paths.contains(&path) {
                        history.paths.push(path);
                    }
                    if history.paths.len() == LIMIT {
                        break;
                    }
                }
            }
            Err(error) => {
                history.unreadable = true;
                history.error = Some(format!("无法读取最近文件：{error:#}"));
            }
        }
        history
    }

    pub fn remember(&mut self, path: &Path) {
        if !path.is_absolute() {
            return;
        }
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        self.change(|paths| {
            paths.retain(|old| *old != path);
            paths.insert(0, path.clone());
            paths.truncate(LIMIT);
        });
    }

    pub fn rename(&mut self, old: &Path, new: &Path) {
        let normalized = normalize_removed_path(old);
        let new = std::fs::canonicalize(new).unwrap_or_else(|_| new.to_owned());
        self.change(|paths| {
            if let Some(index) = paths
                .iter()
                .position(|path| path == old || *path == normalized)
            {
                paths[index] = new.clone();
                let mut seen = std::collections::HashSet::new();
                paths.retain(|path| seen.insert(path.clone()));
            }
        });
    }

    pub fn remove(&mut self, path: &Path) {
        let normalized = normalize_removed_path(path);
        self.change(|paths| paths.retain(|old| old != path && *old != normalized));
    }

    pub fn clear(&mut self) -> Result<()> {
        // Commit before clearing memory, so a failed write cannot claim success.
        if let Err(error) = self.persist_change(|paths| paths.clear(), true) {
            self.error = Some(format!("无法清除最近文件：{error:#}"));
            return Err(error);
        }
        self.paths.clear();
        self.unreadable = false;
        self.error = None;
        Ok(())
    }

    fn change(&mut self, change: impl Fn(&mut Vec<PathBuf>)) {
        change(&mut self.paths);
        if self.unreadable {
            return;
        }
        // Opening/saving a document still succeeds if history storage is unavailable.
        match self.persist_change(change, false) {
            Ok(Some(paths)) => {
                self.paths = paths;
                self.error = None;
            }
            Ok(None) => self.error = None,
            Err(error) => {
                self.error = Some(format!("最近文件仅在本次运行保留，无法保存记录：{error:#}"))
            }
        }
    }

    fn persist_change(
        &self,
        change: impl Fn(&mut Vec<PathBuf>),
        clear: bool,
    ) -> Result<Option<Vec<PathBuf>>> {
        let Some(path) = &self.path else {
            // Isolated GPUI tests use an in-memory session.
            anyhow::ensure!(!self.unreadable, "无法找到配置目录");
            return Ok(None);
        };
        let parent = path.parent().context("最近文件记录缺少目录")?;
        std::fs::create_dir_all(parent)?;
        // The OS may launch independent processes for file associations. Lock a
        // stable sibling (the JSON itself is replaced), then edit the latest data
        // so a stale process cannot overwrite new entries or resurrect a clear.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        let mut paths = if clear {
            Vec::new()
        } else {
            let latest = Self::load_at(path.clone());
            anyhow::ensure!(!latest.unreadable, "{}", latest.error.unwrap_or_default());
            latest.paths
        };
        change(&mut paths);
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(
            &mut file,
            &StoredHistory {
                paths: paths.clone(),
            },
        )?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(Some(paths))
    }
}

fn normalize_removed_path(path: &Path) -> PathBuf {
    // A rename/delete has already removed the file; its parent still resolves
    // Windows short-name aliases and the extended-length prefix consistently.
    path.parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
        .zip(path.file_name())
        .map_or_else(|| path.to_owned(), |(parent, name)| parent.join(name))
}

pub(super) fn display_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    }
}

/// Keep the root and the filename readable, eliding directories in between.
pub(super) fn menu_label(path: &Path, limit: usize) -> String {
    let full = display_path(path);
    let chars = full.chars().collect::<Vec<_>>();
    if chars.len() <= limit || limit < 8 {
        return full;
    }
    let head = (limit / 4).min(18);
    let tail = limit - head - 1;
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_restores_order_duplicates_capacity_and_clear() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("config/recent-files.json");
        let mut history = RecentFiles::load_at(store.clone());
        assert!(history.paths.is_empty());
        let paths = (0..23)
            .map(|i| directory.path().join(format!("中文-{i}.md")))
            .collect::<Vec<_>>();
        for path in &paths {
            history.remember(path);
        }
        history.remember(&paths[10]);
        let mut restored = RecentFiles::load_at(store.clone());
        assert_eq!(restored.paths, history.paths);
        assert_eq!(restored.paths.len(), LIMIT);
        assert_eq!(restored.paths[0], paths[10]);
        assert!(!restored.paths.contains(&paths[0]));
        restored.clear().unwrap();
        assert!(RecentFiles::load_at(store).paths.is_empty());
    }

    #[test]
    fn rename_and_remove_are_persisted_without_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        let old = directory.path().join("old.md");
        let new = directory.path().join("new.md");
        let mut history = RecentFiles::load_at(store.clone());
        history.remember(&new);
        history.remember(&old);
        history.rename(&old, &new);
        assert_eq!(
            RecentFiles::load_at(store.clone()).paths,
            std::slice::from_ref(&new)
        );
        history.remove(&new);
        assert!(RecentFiles::load_at(store).paths.is_empty());
    }

    #[test]
    fn saved_file_aliases_deduplicate_and_moves_update_the_canonical_entry() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        let first = directory.path().join("中文.md");
        let renamed = directory.path().join("重命名.md");
        std::fs::write(&first, "text").unwrap();
        let canonical = std::fs::canonicalize(&first).unwrap();
        let mut history = RecentFiles::load_at(store.clone());
        history.remember(&first);
        history.remember(&canonical);
        assert_eq!(history.paths, [canonical]);
        std::fs::rename(&first, &renamed).unwrap();
        history.rename(&first, &renamed);
        assert_eq!(
            RecentFiles::load_at(store.clone()).paths,
            [std::fs::canonicalize(&renamed).unwrap()]
        );
        std::fs::remove_file(&renamed).unwrap();
        history.remove(&renamed);
        assert!(RecentFiles::load_at(store).paths.is_empty());
    }

    #[test]
    fn load_filters_relative_duplicates_and_keeps_unavailable_absolute_paths() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        let offline = directory.path().join("offline/note.md");
        let mut paths = vec![
            PathBuf::from("relative.md"),
            offline.clone(),
            offline.clone(),
        ];
        paths.extend((0..30).map(|i| directory.path().join(format!("{i}.md"))));
        std::fs::write(
            &store,
            serde_json::to_vec(&StoredHistory { paths }).unwrap(),
        )
        .unwrap();
        let history = RecentFiles::load_at(store);
        assert_eq!(history.paths.len(), LIMIT);
        assert_eq!(history.paths[0], offline);
        assert_eq!(
            history
                .paths
                .iter()
                .filter(|path| **path == offline)
                .count(),
            1
        );
        assert!(history.paths.iter().all(|path| path.is_absolute()));
    }

    #[test]
    fn independent_instances_preserve_new_entries_and_do_not_restore_cleared_history() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        let first = directory.path().join("first.md");
        let second = directory.path().join("second.md");
        let third = directory.path().join("third.md");
        let mut a = RecentFiles::load_at(store.clone());
        let mut b = RecentFiles::load_at(store.clone());
        a.remember(&first);
        b.remember(&second);
        assert_eq!(RecentFiles::load_at(store.clone()).paths, [second, first]);
        a.clear().unwrap();
        b.remember(&third);
        assert_eq!(RecentFiles::load_at(store).paths, [third]);
    }

    #[test]
    fn corrupt_store_is_preserved_until_explicit_clear() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        std::fs::write(&store, "broken").unwrap();
        let mut history = RecentFiles::load_at(store.clone());
        assert!(history.error.is_some());
        history.remember(&directory.path().join("note.md"));
        assert_eq!(std::fs::read_to_string(&store).unwrap(), "broken");
        assert_eq!(history.paths.len(), 1);
        history.clear().unwrap();
        let restored = RecentFiles::load_at(store);
        assert!(restored.paths.is_empty());
        assert!(restored.error.is_none());
    }

    #[test]
    fn failed_clear_keeps_history_and_existing_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = directory.path().join("recent.json");
        let mut history = RecentFiles::load_at(store.clone());
        let note = directory.path().join("note.md");
        history.remember(&note);
        let blocker = directory.path().join("blocked");
        std::fs::write(&blocker, "unchanged").unwrap();
        history.path = Some(blocker.join("recent.json"));
        assert!(history.clear().is_err());
        assert_eq!(history.paths, std::slice::from_ref(&note));
        assert_eq!(RecentFiles::load_at(store).paths, [note]);
        assert_eq!(std::fs::read_to_string(blocker).unwrap(), "unchanged");
    }

    #[test]
    fn labels_hide_extended_prefix_and_elide_unicode_safely() {
        assert_eq!(
            display_path(Path::new(r"\\?\D:\notes\中文.md")),
            r"D:\notes\中文.md"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\notes\文档.md")),
            r"\\server\notes\文档.md"
        );
        let full = format!("D:\\{}\\文件🌱.md", "很长的目录\\".repeat(20));
        let label = menu_label(Path::new(&full), 64);
        assert_eq!(label.chars().count(), 64);
        assert!(label.starts_with("D:\\"));
        assert!(label.ends_with("文件🌱.md"));
        assert!(label.contains('…'));
        assert_eq!(
            menu_label(Path::new("/notes/short.md"), 64),
            "/notes/short.md"
        );
    }
}
