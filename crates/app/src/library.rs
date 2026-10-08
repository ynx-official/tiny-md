//! Markdown file discovery and non-overwriting file operations.
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub preview: String,
    pub is_dir: bool,
}

pub fn summary(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("```") && !line.starts_with("---"))
        .take(3)
        .map(|line| line.trim_start_matches(['#', '>', ' ', '*']))
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(100)
        .collect()
}

fn markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["md", "markdown", "mdown"]
            .iter()
            .any(|name| extension.eq_ignore_ascii_case(name))
    })
}

fn scan(root: &Path, depth: usize, entries: &mut Vec<Entry>) -> io::Result<()> {
    if depth > 32 || entries.len() >= 1024 {
        return Ok(());
    }
    let mut paths = fs::read_dir(root)?.collect::<io::Result<Vec<_>>>()?;
    paths.sort_by_key(|entry| {
        (
            !entry.path().is_dir(),
            entry.file_name().to_ascii_lowercase(),
        )
    });
    for entry in paths {
        if entries.len() >= 1024 {
            break;
        }
        let kind = entry.file_type()?;
        let path = entry.path();
        // Never recurse through links or hidden/build directories.
        if kind.is_dir() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "target" || name == "node_modules"
            {
                continue;
            }
            entries.push(Entry {
                path: path.clone(),
                preview: String::new(),
                is_dir: true,
            });
            scan(&path, depth + 1, entries)?;
        } else if kind.is_file() && markdown(&path) {
            entries.push(file_entry(path));
        }
    }
    Ok(())
}

fn file_entry(path: PathBuf) -> Entry {
    let mut bytes = Vec::new();
    let preview =
        match fs::File::open(&path).and_then(|file| file.take(8192).read_to_end(&mut bytes)) {
            Ok(_) => summary(&String::from_utf8_lossy(&bytes)),
            Err(_) => "无法读取文档摘要".into(),
        };
    Entry {
        path,
        preview,
        is_dir: false,
    }
}

pub fn load(root: Option<&Path>, mut recent: Vec<PathBuf>) -> io::Result<Vec<Entry>> {
    let mut entries = vec![];
    if let Some(root) = root {
        scan(&fs::canonicalize(root)?, 0, &mut entries)?;
    } else {
        recent.sort();
        recent.dedup();
        entries.extend(
            recent
                .into_iter()
                .filter(|path| path.is_file())
                .map(file_entry),
        );
    }
    Ok(entries)
}

pub fn visible<'a>(
    entries: &'a [Entry],
    query: &str,
    collapsed: &BTreeSet<PathBuf>,
    tree: bool,
) -> Vec<&'a Entry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| {
            if !tree && entry.is_dir {
                return false;
            }
            if query.is_empty() {
                return !tree
                    || !collapsed
                        .iter()
                        .any(|folder| entry.path != *folder && entry.path.starts_with(folder));
            }
            let matches = |item: &Entry| {
                item.path.to_string_lossy().to_lowercase().contains(&query)
                    || item.preview.to_lowercase().contains(&query)
            };
            matches(entry)
                || (tree
                    && entry.is_dir
                    && entries.iter().any(|child| {
                        !child.is_dir && child.path.starts_with(&entry.path) && matches(child)
                    }))
        })
        .collect()
}

pub fn named_path(directory: &Path, name: &str) -> io::Result<PathBuf> {
    let name = name.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.ends_with('.')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "请输入有效的文件名，不要包含路径或特殊字符。",
        ));
    }
    #[cfg(target_os = "windows")]
    {
        // Device names stay reserved even with an extension. Win32 also ignores
        // spaces before the extension, and accepts superscript digits as ports.
        let base = name
            .split('.')
            .next()
            .unwrap_or_default()
            .trim_end()
            .to_uppercase();
        let port = base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"));
        if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || port.is_some_and(|number| {
                matches!(
                    number,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "此文件名由 Windows 保留，请使用其他名称。",
            ));
        }
    }
    let mut path = directory.join(name);
    if path.extension().is_none() {
        path.set_extension("md");
    }
    if !markdown(&path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "文件扩展名应为 .md、.markdown 或 .mdown。",
        ));
    }
    Ok(path)
}

pub fn create_file(path: &Path) -> io::Result<PathBuf> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.sync_all()?;
    fs::canonicalize(path)
}

pub fn duplicate_file(path: &Path) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("文件没有所在文件夹"))?;
    for number in 1..=10000 {
        let mut name = path.file_stem().unwrap_or_default().to_os_string();
        name.push(if number == 1 {
            " 副本".into()
        } else {
            format!(" 副本 {number}")
        });
        if let Some(extension) = path.extension() {
            name.push(".");
            name.push(extension);
        }
        let target = parent.join(name);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            let mut source = fs::File::open(path)?;
            io::copy(&mut source, &mut output)?;
            output.flush()?;
            output.sync_all()?;
            output.set_permissions(source.metadata()?.permissions())?;
            fs::canonicalize(&target)
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(&target);
        }
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "副本名称已用尽",
    ))
}

pub fn rename_file(path: &Path, target: &Path) -> io::Result<PathBuf> {
    if path == target {
        return Ok(path.to_owned());
    }
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(io::Error::other("请先在文件管理器中处理符号链接。"));
    }
    // Creating the destination link is atomic and fails if a target already exists.
    // Unlike rename(), it cannot replace another document on Unix.
    fs::hard_link(path, target)?;
    if let Err(error) = fs::remove_file(path) {
        let _ = fs::remove_file(target);
        return Err(error);
    }
    fs::canonicalize(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_device_names_are_rejected_before_filesystem_operations() {
        for invalid in [
            "CON",
            "nul.md",
            "PRN.markdown",
            "AUX",
            "COM1.md",
            "LPT9.mdown",
            "con .md",
            "COM¹.md",
        ] {
            assert!(
                named_path(Path::new("notes"), invalid).is_err(),
                "accepted {invalid}"
            );
        }
        assert_eq!(
            named_path(Path::new("notes"), "console.md").unwrap(),
            Path::new("notes").join("console.md")
        );
    }
    #[test]
    fn file_operations_never_replace_existing_documents_and_preserve_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("中文.md");
        let bytes = b"\xef\xbb\xbf# title\r\n\r\ntext\r\n";
        fs::write(&original, bytes).unwrap();
        let first = duplicate_file(&original).unwrap();
        let second = duplicate_file(&original).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), bytes);
        assert!(create_file(&original).is_err());
        assert!(rename_file(&original, &first).is_err());
        assert_eq!(fs::read(&original).unwrap(), bytes);
        let renamed = rename_file(&original, &dir.path().join("新名字.md")).unwrap();
        assert!(!original.exists());
        assert_eq!(fs::read(renamed).unwrap(), bytes);
    }
    #[test]
    fn tree_search_keeps_ancestors_and_ignores_collapsed_folders() {
        let dir = tempfile::tempdir().unwrap();
        let folder = fs::canonicalize(dir.path()).unwrap().join("notes");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("child.md"), "unique searchable text").unwrap();
        fs::write(dir.path().join("root.md"), "root").unwrap();
        fs::write(dir.path().join("ignore.txt"), "text").unwrap();
        let entries = load(Some(dir.path()), vec![]).unwrap();
        let collapsed = BTreeSet::from([folder]);
        assert_eq!(visible(&entries, "", &collapsed, true).len(), 2);
        assert_eq!(visible(&entries, "unique", &collapsed, true).len(), 2);
        assert_eq!(visible(&entries, "", &collapsed, false).len(), 2);
        for invalid in ["", ".", "..", "../escape", "a/b", "bad.txt"] {
            assert!(named_path(dir.path(), invalid).is_err());
        }
        assert_eq!(
            named_path(dir.path(), "新文档").unwrap(),
            dir.path().join("新文档.md")
        );
    }
}
