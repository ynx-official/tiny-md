//! Coalesced file events with a byte-read audit when events are unavailable.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::{self, Receiver};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const AUDIT_INTERVAL: Duration = Duration::from_secs(5);

pub struct DiskWatch {
    path: Option<PathBuf>,
    last_read: Instant,
    pending: bool,
    watcher: Option<RecommendedWatcher>,
    events: Option<Receiver<()>>,
}
impl Default for DiskWatch {
    fn default() -> Self {
        Self {
            path: None,
            last_read: Instant::now(),
            pending: false,
            watcher: None,
            events: None,
        }
    }
}
impl DiskWatch {
    /// Keep a detected change pending when typing or a new save invalidates an
    /// asynchronous read. The next available tick must read the latest bytes.
    pub fn retry(&mut self) {
        self.pending = self.path.is_some();
    }
    pub fn set_path(&mut self, path: Option<&Path>) {
        if self.path.as_deref() == path {
            return;
        }
        self.watcher = None;
        self.events = None;
        self.path = path.map(Path::to_owned);
        self.pending = path.is_some();
        let Some(path) = path else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let target = event_path(path);
        // Filter before enqueueing and cap the queue: bursts cannot allocate an
        // unbounded backlog. One pending event is enough to read the latest bytes.
        let (send, receive) = mpsc::sync_channel(1);
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let changed = match result {
                Ok(event) => {
                    !matches!(event.kind, EventKind::Access(_))
                        && (event.paths.is_empty()
                            || event.paths.iter().any(|path| event_path(path) == target))
                }
                Err(_) => true,
            };
            if changed {
                let _ = send.try_send(());
            }
        });
        if let Ok(mut watcher) = watcher {
            // Watching the directory survives the atomic replacement used by
            // other editors. Failed/missing watches fall back to byte audits.
            if watcher.watch(parent, RecursiveMode::NonRecursive).is_ok() {
                self.watcher = Some(watcher);
                self.events = Some(receive);
            }
        }
    }
    pub fn needs_read(&mut self, now: Instant, available: bool) -> bool {
        if let Some(events) = &self.events {
            while events.try_recv().is_ok() {
                self.pending = true;
            }
        }
        if !available
            || self.path.is_none()
            || (!self.pending && now.saturating_duration_since(self.last_read) < AUDIT_INTERVAL)
        {
            return false;
        }
        self.last_read = now;
        self.pending = false;
        true
    }
}

fn event_path(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(
            path.to_string_lossy()
                .trim_start_matches("\\\\?\\")
                .to_lowercase(),
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_ticks_skip_reads_and_busy_state_retains_pending_events() {
        let mut watch = DiskWatch::default();
        watch.set_path(Some(Path::new("note.md")));
        let now = Instant::now();
        assert!(!watch.needs_read(now, false));
        assert!(watch.needs_read(now, true));
        assert!(!watch.needs_read(now + Duration::from_secs(1), true));
        assert!(watch.needs_read(now + AUDIT_INTERVAL, true));
        watch.retry();
        assert!(!watch.needs_read(now + AUDIT_INTERVAL, false));
        assert!(watch.needs_read(now + AUDIT_INTERVAL, true));
        watch.set_path(None);
        assert!(!watch.needs_read(now + AUDIT_INTERVAL * 2, true));
    }
    #[test]
    fn native_events_detect_overwrites_atomic_replacements_and_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文.md");
        std::fs::write(&path, "before").unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let mut watch = DiskWatch::default();
        watch.set_path(Some(&path));
        assert!(watch.needs_read(Instant::now(), true));
        std::fs::write(&path, "after!").unwrap();
        wait_for_event(&mut watch);
        let mut replacement = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        use std::io::Write;
        replacement.write_all(b"atomic").unwrap();
        replacement.persist(&path).unwrap();
        wait_for_event(&mut watch);
        std::fs::remove_file(&path).unwrap();
        wait_for_event(&mut watch);
    }
    fn wait_for_event(watch: &mut DiskWatch) {
        let start = Instant::now();
        loop {
            if watch.needs_read(Instant::now(), true) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "native event did not trigger a read"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
