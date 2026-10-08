use super::{Document, Intent, Path, Session, TinyMd, library};
use gpui::{App, Context, PromptLevel, Task, Window};
use tiny_md_document::{DiskWatch, SyncError};

pub(super) const CONFLICT_NOTICE: &str = "同步冲突：本地内容已保留";

fn restore_conflict_notice(active: &mut bool, status: &mut String, error: &mut bool) -> bool {
    if !*active || status != CONFLICT_NOTICE || !*error {
        *active = true;
        *status = CONFLICT_NOTICE.into();
        *error = true;
        true
    } else {
        false
    }
}

impl TinyMd {
    pub(super) fn start_disk_sync(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut watch = DiskWatch::default();
            loop {
                executor.timer(std::time::Duration::from_millis(150)).await;
                let snapshot = this.update_in(cx, |this, _, cx| {
                    (
                        this.document.clone(),
                        !this.busy
                            && !this.editor.read(cx).is_composing()
                            && this.document.path().is_some(),
                    )
                });
                let (document, available) = match snapshot {
                    Ok(snapshot) => snapshot,
                    Err(_) => break,
                };
                watch.set_path(document.path());
                if !watch.needs_read(std::time::Instant::now(), available) {
                    continue;
                }
                let read_document = document.clone();
                let change = executor
                    .spawn(async move { read_document.read_external_change() })
                    .await;
                let change = match change {
                    Ok(Some(change)) => change,
                    Ok(None) => continue,
                    Err(error) => {
                        let _ = this.update_in(cx, |this, _, cx| {
                            if this.can_sync(&document, cx) {
                                let status = format!("同步失败：{error}（当前编辑已保留）");
                                if this.status != status {
                                    this.status = status;
                                    this.error = true;
                                    cx.notify();
                                }
                            }
                        });
                        continue;
                    }
                };
                let local = match this.update_in(cx, |this, _, cx| {
                    this.can_sync(&document, cx)
                        .then(|| this.editor.read(cx).text())
                }) {
                    Ok(Some(local)) => local,
                    Ok(None) => {
                        watch.retry();
                        continue;
                    }
                    Err(_) => break,
                };
                let merge_document = document.clone();
                let merge_local = local.clone();
                let result = executor
                    .spawn(async move { merge_document.reconcile(change, &merge_local) })
                    .await;
                let updated = this.update_in(cx, |this, _, cx| {
                    // Reads/merges run off the UI thread. Never apply them over
                    // typing, composition, a save, or a subsequently opened file.
                    if !this.can_sync(&document, cx) || this.editor.read(cx).text() != local {
                        return false;
                    }
                    match result {
                        Ok((document, text)) => {
                            this.adopt_synced(document, &text, "已同步其他应用的修改", cx)
                        }
                        Err(SyncError::Conflict) => this.mark_sync_conflict(cx),
                        Err(error) => this.fail(format!("同步失败：{error}"), cx),
                    }
                    true
                });
                match updated {
                    Ok(false) => watch.retry(),
                    Ok(true) => {}
                    Err(_) => break,
                }
            }
        })
    }

    fn can_sync(&self, snapshot: &Document, cx: &App) -> bool {
        !self.busy && !self.editor.read(cx).is_composing() && self.document.same_baseline(snapshot)
    }

    pub(super) fn adopt_synced(
        &mut self,
        document: Document,
        text: &str,
        status: &str,
        cx: &mut Context<Self>,
    ) {
        self.document = document;
        self.external_conflict = false;
        self.editor.update(cx, |editor, cx| {
            editor.apply_external_text(text, cx);
        });
        self.dirty = self.document.is_dirty(text);
        self.update_analysis(text);
        self.status = status.into();
        self.error = false;
        if let Some(entry) = self
            .documents
            .iter_mut()
            .find(|entry| Some(entry.path.as_path()) == self.document.path())
        {
            entry.preview = library::summary(text);
        }
        cx.notify();
    }

    pub(super) fn mark_sync_conflict(&mut self, cx: &mut Context<Self>) {
        if restore_conflict_notice(
            &mut self.external_conflict,
            &mut self.status,
            &mut self.error,
        ) {
            cx.notify();
        }
    }

    pub(super) fn resolve_external_conflict(
        &mut self,
        after: Option<Intent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.finish_input(cx);
        cx.global_mut::<Session>().quitting = false;
        self.set_busy(true, cx);
        let answer = window.prompt(
            PromptLevel::Warning,
            "本地和其他应用修改了同一处内容",
            Some("重叠修改无法自动合并。可以另存本地副本，或同步磁盘版本。同步后可在编辑器中撤销，恢复本地内容。"),
            &["另存为本地副本", "同步磁盘版本", "取消"], cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                this.editor.read(cx).focus_handle().focus(window);
                match answer {
                    Ok(0) => {
                        if matches!(after, Some(Intent::Quit)) {
                            cx.global_mut::<Session>().quitting = true;
                        }
                        this.save(true, after, window, cx);
                    }
                    Ok(1) => this.reload_disk_version(window, cx),
                    _ => {}
                }
            });
        })
        .detach();
    }

    fn reload_disk_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.document.path().map(Path::to_owned) else {
            return;
        };
        self.set_busy(true, cx);
        let task = cx
            .background_executor()
            .spawn(async move { Document::open(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.set_busy(false, cx);
                match result {
                    Ok((document, text)) => {
                        this.adopt_synced(document, &text, "已同步磁盘版本，可撤销恢复本地内容", cx)
                    }
                    Err(error) => this.fail(format!("同步失败：{error}（当前编辑已保留）"), cx),
                }
                this.editor.read(cx).focus_handle().focus(window);
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{CONFLICT_NOTICE, restore_conflict_notice};

    #[::core::prelude::v1::test]
    fn repeated_conflicting_saves_restore_notice_instead_of_staying_at_saving() {
        let mut active = true;
        let mut status = "正在保存……".to_owned();
        let mut error = true;
        assert!(restore_conflict_notice(
            &mut active,
            &mut status,
            &mut error
        ));
        assert!(active && error);
        assert_eq!(status, CONFLICT_NOTICE);
        assert!(!restore_conflict_notice(
            &mut active,
            &mut status,
            &mut error
        ));
    }
}
