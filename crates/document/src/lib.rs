//! File state independent of the editor. Live text remains owned by the view.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default)]
enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

#[derive(Clone, Debug)]
pub struct Document {
    path: Option<PathBuf>,
    saved_text: String,
    disk_snapshot: Option<Vec<u8>>,
    line_ending: LineEnding,
    bom: bool,
}

impl Document {
    pub fn untitled(text: &str) -> Self {
        Self {
            path: None,
            saved_text: text.into(),
            disk_snapshot: None,
            line_ending: LineEnding::Lf,
            bom: false,
        }
    }

    pub fn open(path: &Path) -> io::Result<(Self, String)> {
        let path = fs::canonicalize(path)?;
        let bytes = fs::read(&path)?;
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let source = std::str::from_utf8(if bom { &bytes[3..] } else { &bytes })
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let line_ending = if source.contains("\r\n") && !source.replace("\r\n", "").contains('\n') {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        let text = source.replace("\r\n", "\n").replace('\r', "\n");
        let document = Self {
            path: Some(path),
            saved_text: text.clone(),
            disk_snapshot: Some(bytes),
            line_ending,
            bom,
        };
        Ok((document, text))
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// Update the path after a successful filesystem move, preserving the saved
    /// snapshot, encoding and dirty baseline. The editor keeps its undo history.
    pub fn retarget_after_move(&mut self, old: &Path, new: &Path) {
        if self.path.as_deref() == Some(old) {
            self.path = Some(new.to_owned());
        }
    }
    pub fn is_dirty(&self, text: &str) -> bool {
        self.saved_text != text
    }

    pub fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名.md".into())
    }

    pub fn save(&mut self, text: &str) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "请选择保存位置"))?;
        // Best-effort external-change detection. Never silently overwrite a
        // changed/deleted file. Save As is the explicit way to keep a new copy.
        let current = fs::read(&path)?;
        if Some(&current) != self.disk_snapshot.as_ref() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "文件已被其他程序修改，请另存为，或重新打开文件后合并修改。",
            ));
        }
        if !self.is_dirty(text) {
            return Ok(());
        }
        self.write_to(&path, text)
    }

    pub fn save_as(&mut self, path: &Path, text: &str) -> io::Result<()> {
        let path = if path.exists() {
            fs::canonicalize(path)?
        } else {
            path.to_path_buf()
        };
        // Same target must still go through conflict detection.
        if self.path.as_ref() == Some(&path) {
            return self.save(text);
        }
        self.write_to(&path, text)
    }

    fn write_to(&mut self, path: &Path, text: &str) -> io::Result<()> {
        let content = match self.line_ending {
            LineEnding::Lf => text.to_owned(),
            LineEnding::CrLf => text.replace('\n', "\r\n"),
        };
        let mut bytes = Vec::with_capacity(content.len() + 3);
        if self.bom {
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        bytes.extend_from_slice(content.as_bytes());
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        if let Ok(metadata) = fs::metadata(path) {
            temp.as_file().set_permissions(metadata.permissions())?;
        }
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| e.error)?;
        // Record success only after the atomic replacement succeeded.
        self.path = Some(path.to_path_buf());
        self.disk_snapshot = Some(bytes);
        self.saved_text = text.into();
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub line: usize,
    pub level: u8,
    pub text: String,
}

/// Outline of ATX headings, excluding code fences and YAML front matter.
pub fn outline(text: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut frontmatter = false;
    for (line, source) in text.lines().enumerate() {
        if line == 0 && source == "---" {
            frontmatter = true;
            continue;
        }
        if frontmatter {
            if source == "---" || source == "..." {
                frontmatter = false;
            }
            continue;
        }
        let trimmed = source.trim_start_matches(' ');
        if source.len() - trimmed.len() > 3 {
            continue;
        }
        let first = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|&c| c == first).count();
        if let Some((ch, len)) = fence {
            if first == ch && count >= len && trimmed[count..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if matches!(first, '`' | '~') && count >= 3 {
            fence = Some((first, count));
            continue;
        }
        if first == '#' && (1..=6).contains(&count) {
            let content = &trimmed[count..];
            if content.is_empty() || content.starts_with([' ', '\t']) {
                let mut title = content.trim();
                let without_hashes = title.trim_end_matches('#');
                if without_hashes.ends_with([' ', '\t']) {
                    title = without_hashes.trim_end();
                }
                headings.push(Heading {
                    line,
                    level: count as u8,
                    text: title.to_owned(),
                });
            }
        }
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retargeted_document_keeps_dirty_baseline_encoding_and_conflict_checks() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let old = root.join("old.md");
        let new = root.join("renamed.md");
        fs::write(&old, b"\xef\xbb\xbf# title\r\n").unwrap();
        let (mut document, _) = Document::open(&old).unwrap();
        fs::rename(&old, &new).unwrap();
        document.retarget_after_move(&old, &new);
        assert_eq!(document.path(), Some(new.as_path()));
        assert!(document.is_dirty("# changed\n"));
        document.save("# changed\n").unwrap();
        assert_eq!(fs::read(&new).unwrap(), b"\xef\xbb\xbf# changed\r\n");
        fs::write(&new, "external edit").unwrap();
        assert!(document.save("next edit").is_err());
        assert_eq!(fs::read(&new).unwrap(), b"external edit");
    }

    #[test]
    fn saves_unicode_and_preserves_bom_crlf_and_final_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文.md");
        fs::write(&path, b"\xef\xbb\xbf# title\r\n\r\ntext\r\n").unwrap();
        let (mut doc, text) = Document::open(&path).unwrap();
        assert_eq!(text, "# title\n\ntext\n");
        doc.save("# 中文\n\n🌱\n").unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            "\u{feff}# 中文\r\n\r\n🌱\r\n".as_bytes()
        );
        assert!(!doc.is_dirty("# 中文\n\n🌱\n"));
    }

    #[test]
    fn external_changes_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "original").unwrap();
        let (mut doc, _) = Document::open(&path).unwrap();
        fs::write(&path, "external").unwrap();
        assert_eq!(
            doc.save("local").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(doc.is_dirty("local"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        let copy = dir.path().join("copy.md");
        doc.save_as(&copy, "local").unwrap();
        assert_eq!(fs::read_to_string(copy).unwrap(), "local");
    }

    #[test]
    fn failed_save_keeps_document_unsaved_and_success_tracks_undo() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::untitled("");
        assert!(
            doc.save_as(&dir.path().join("missing/note.md"), "draft")
                .is_err()
        );
        assert!(doc.path().is_none());
        assert!(doc.is_dirty("draft"));
        doc.save_as(&dir.path().join("note.md"), "draft").unwrap();
        assert!(!doc.is_dirty("draft"));
        assert!(doc.is_dirty(""));
    }

    #[test]
    fn outline_ignores_fences_metadata_and_non_headings() {
        let headings = outline(
            "---\n# metadata\n---\n# 正文\n```md\n# example\n```\n## 子标题 ##\n#not-a-heading\n    # indented\n###",
        );
        assert_eq!(
            headings
                .iter()
                .map(|h| (h.line, h.level, h.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(3, 1, "正文"), (7, 2, "子标题"), (10, 3, "")]
        );
    }
}
