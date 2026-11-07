use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Notes are read only up to this many bytes when indexing the vault: enough for frontmatter and headings.
const SNIPPET_BYTES: u64 = 4096;
/// Safety valve for huge vaults; the placement index is built from this many notes at most.
const MAX_SCANNED_NOTES: usize = 20_000;
/// Upper bound for the plugin's `data.json` (it holds settings and learned folder preferences).
const MAX_PLUGIN_DATA_BYTES: u64 = 2 * 1024 * 1024;
const PLUGIN_DATA_PATH: &str = ".obsidian/plugins/archivarious/data.json";
/// The user's section list, shared with the plugin. Same path as `SECTIONS_FILE` in `@archivarious/core`.
const SECTIONS_PATH: &str = "ArchiVarious/Разделы.md";
/// The section list is a short note; anything larger is not ours to rewrite.
const MAX_SECTIONS_BYTES: u64 = 256 * 1024;

/// Writes one capture file into `<vault>/<inbox_folder>/<file_name>`.
/// The contents are produced by `@archivarious/core` (inbox protocol); Rust only guards the path.
#[tauri::command]
pub fn write_capture(
    vault_path: String,
    inbox_folder: String,
    file_name: String,
    contents: String,
) -> Result<String, String> {
    let target = resolve_target(Path::new(&vault_path), &inbox_folder, &file_name)?;
    create_new_file(&target, &contents).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

/// Writes a sorted note into `<vault>/<folder>/`, picking `Name.md`, `Name 1.md`, … so an existing
/// note is never overwritten. Returns the file name that was used.
#[tauri::command]
pub fn write_note_unique(
    vault_path: String,
    folder: String,
    file_name: String,
    contents: String,
) -> Result<String, String> {
    let vault = Path::new(&vault_path);
    resolve_target(vault, &folder, &file_name)?;
    let stem = file_name.strip_suffix(".md").unwrap_or(&file_name);
    for n in 0..1000 {
        let candidate = if n == 0 {
            file_name.clone()
        } else {
            format!("{stem} {n}.md")
        };
        let target = resolve_target(vault, &folder, &candidate)?;
        match create_new_file(&target, &contents) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("too many notes with the same name".into())
}

/// Creates the note only if it does not exist yet (folder hub notes). Returns whether it was created.
#[tauri::command]
pub fn ensure_note(
    vault_path: String,
    folder: String,
    file_name: String,
    contents: String,
) -> Result<bool, String> {
    let target = resolve_target(Path::new(&vault_path), &folder, &file_name)?;
    match create_new_file(&target, &contents) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct NoteSnippet {
    /// Vault-relative, forward slashes.
    pub path: String,
    pub text: String,
}

/// Reads the beginning of every Markdown note (outside dot-folders and the inbox) so the widget can
/// suggest folders without Obsidian running. Symlinks are not followed.
#[tauri::command]
pub fn scan_vault(vault_path: String, inbox_folder: String) -> Result<Vec<NoteSnippet>, String> {
    let vault = Path::new(&vault_path);
    check_vault(vault)?;
    let inbox = inbox_folder.trim_matches('/').to_string();
    let mut notes = Vec::new();
    scan_dir(vault, "", &inbox, &mut notes);
    Ok(notes)
}

/// Raw contents of the plugin's `data.json`, if the plugin is installed in this vault.
#[tauri::command]
pub fn read_plugin_data(vault_path: String) -> Result<Option<String>, String> {
    let vault = Path::new(&vault_path);
    check_vault(vault)?;
    let file = match fs::File::open(vault.join(PLUGIN_DATA_PATH)) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut text = String::new();
    file.take(MAX_PLUGIN_DATA_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(Some(text))
}

/// The section list note, if it exists.
#[tauri::command]
pub fn read_sections(vault_path: String) -> Result<Option<String>, String> {
    let vault = Path::new(&vault_path);
    check_vault(vault)?;
    let file = match fs::File::open(vault.join(SECTIONS_PATH)) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut text = String::new();
    file.take(MAX_SECTIONS_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(Some(text))
}

/// Rewrites the section list note and creates the folders of the given sections. This is the only command
/// that overwrites a file, and only this one fixed file; section folders go through the usual path guard.
#[tauri::command]
pub fn write_sections(
    vault_path: String,
    contents: String,
    folders: Vec<String>,
) -> Result<(), String> {
    let vault = Path::new(&vault_path);
    let mut dirs = Vec::new();
    for folder in &folders {
        // Validated as `<folder>/x.md`, then only the folder is created.
        let probe = resolve_target(vault, folder, "x.md")?;
        dirs.extend(probe.parent().map(Path::to_path_buf));
    }
    let target = vault.join(SECTIONS_PATH);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target, contents).map_err(|e| e.to_string())?;
    for dir in dirs {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Inbox captures are small; a larger file in the inbox folder is not one of ours.
const MAX_CAPTURE_BYTES: u64 = 256 * 1024;

#[derive(Serialize, Debug, PartialEq)]
pub struct InboxFile {
    /// File name inside the inbox folder.
    pub name: String,
    pub text: String,
}

/// The capture files waiting in the inbox (only files written in the inbox protocol), so the widget can
/// notice due surveys while Obsidian is closed.
#[tauri::command]
pub fn list_inbox(vault_path: String, inbox_folder: String) -> Result<Vec<InboxFile>, String> {
    let dir = resolve_target(Path::new(&vault_path), &inbox_folder, "x.md")?
        .parent()
        .map(Path::to_path_buf)
        .ok_or("bad inbox folder")?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().is_ok_and(|t| t.is_file()) || !name.ends_with(".md") {
            continue;
        }
        if let Some(text) = read_capture(&entry.path()) {
            files.push(InboxFile { name, text });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// Replaces a capture in the inbox (a snoozed survey gets a new time). Only an existing capture file
/// can be replaced, and only by another capture.
#[tauri::command]
pub fn replace_inbox_capture(
    vault_path: String,
    inbox_folder: String,
    file_name: String,
    contents: String,
) -> Result<(), String> {
    let target = resolve_target(Path::new(&vault_path), &inbox_folder, &file_name)?;
    if read_capture(&target).is_none() || !is_capture(&contents) {
        return Err("not an inbox capture".into());
    }
    fs::write(&target, contents).map_err(|e| e.to_string())
}

/// Removes a capture from the inbox after it was filed. Returns `false` if it was already gone (for example
/// the plugin sorted it first). Only capture files can be removed.
#[tauri::command]
pub fn remove_inbox_capture(
    vault_path: String,
    inbox_folder: String,
    file_name: String,
) -> Result<bool, String> {
    let target = resolve_target(Path::new(&vault_path), &inbox_folder, &file_name)?;
    if !target.exists() {
        return Ok(false);
    }
    if read_capture(&target).is_none() {
        return Err("not an inbox capture".into());
    }
    match fs::remove_file(&target) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// `archivarious: <version>` as the first frontmatter field marks a file written in the inbox protocol.
fn is_capture(text: &str) -> bool {
    let text = text.replace("\r\n", "\n");
    text.starts_with("---\narchivarious:")
}

fn read_capture(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_CAPTURE_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_CAPTURE_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    is_capture(&text).then_some(text)
}

fn scan_dir(dir: &Path, relative: &str, inbox: &str, notes: &mut Vec<NoteSnippet>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if notes.len() >= MAX_SCANNED_NOTES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !inbox.is_empty() && path == inbox {
                continue;
            }
            scan_dir(&entry.path(), &path, inbox, notes);
        } else if kind.is_file() && name.to_lowercase().ends_with(".md") {
            if let Some(text) = read_head(&entry.path()) {
                notes.push(NoteSnippet { path, text });
            }
        }
    }
}

fn read_head(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(SNIPPET_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn create_new_file(target: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // `create_new` never overwrites an existing note.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    file.write_all(contents.as_bytes())
}

fn check_vault(vault: &Path) -> Result<(), String> {
    if !vault.is_absolute() || !vault.is_dir() {
        return Err(format!("vault folder not found: {}", vault.display()));
    }
    Ok(())
}

fn resolve_target(vault: &Path, folder: &str, file_name: &str) -> Result<PathBuf, String> {
    check_vault(vault)?;
    let folder = Path::new(folder);
    if !folder
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("folder must be a relative path inside the vault".into());
    }
    // The vault's own settings folder is off limits, whatever the caller asks for.
    if folder
        .components()
        .next()
        .is_some_and(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Err("folder must not be a hidden folder".into());
    }
    let name = Path::new(file_name);
    if name.components().count() != 1 || name.extension().and_then(|e| e.to_str()) != Some("md") {
        return Err("file name must be a single .md file name".into());
    }
    Ok(vault.join(folder).join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(tag: &str) -> PathBuf {
        let vault =
            std::env::temp_dir().join(format!("archivarious-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&vault);
        fs::create_dir_all(&vault).unwrap();
        vault
    }

    fn s(v: &Path) -> String {
        v.to_string_lossy().into_owned()
    }

    #[test]
    fn rejects_paths_escaping_the_vault() {
        let vault = std::env::temp_dir();
        assert!(resolve_target(&vault, "../outside", "a.md").is_err());
        assert!(resolve_target(&vault, "/abs", "a.md").is_err());
        assert!(resolve_target(&vault, ".obsidian", "a.md").is_err());
        assert!(resolve_target(&vault, "Inbox", "../a.md").is_err());
        assert!(resolve_target(&vault, "Inbox", "a.txt").is_err());
        assert!(resolve_target(Path::new("relative"), "Inbox", "a.md").is_err());
    }

    #[test]
    fn writes_into_the_inbox_without_overwriting() {
        let vault = temp_vault("inbox");
        let args = || {
            (
                s(&vault),
                "ArchiVarious/Inbox".to_string(),
                "x.md".to_string(),
                "hi".to_string(),
            )
        };

        let (v, i, f, c) = args();
        let written = write_capture(v, i, f, c).unwrap();
        assert_eq!(fs::read_to_string(&written).unwrap(), "hi");

        let (v, i, f, c) = args();
        assert!(write_capture(v, i, f, c).is_err());
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn picks_a_free_name_for_sorted_notes() {
        let vault = temp_vault("unique");
        let write = |text: &str| {
            write_note_unique(s(&vault), "Learning".into(), "Idea.md".into(), text.into()).unwrap()
        };
        assert_eq!(write("one"), "Idea.md");
        assert_eq!(write("two"), "Idea 1.md");
        assert_eq!(write("three"), "Idea 2.md");
        assert_eq!(
            fs::read_to_string(vault.join("Learning/Idea.md")).unwrap(),
            "one"
        );
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn creates_hub_notes_only_once() {
        let vault = temp_vault("hub");
        let ensure = |text: &str| {
            ensure_note(
                s(&vault),
                "Learning".into(),
                "Learning.md".into(),
                text.into(),
            )
            .unwrap()
        };
        assert!(ensure("# Learning\n"));
        assert!(!ensure("overwritten?"));
        assert_eq!(
            fs::read_to_string(vault.join("Learning/Learning.md")).unwrap(),
            "# Learning\n"
        );
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn scans_notes_but_not_hidden_folders_the_inbox_or_other_files() {
        let vault = temp_vault("scan");
        for (path, text) in [
            ("Learning/Memory.md", "# Memory #study"),
            ("Learning/Deep/Loci.md", "loci"),
            ("Learning/image.png", "not a note"),
            ("Root.MD", "root"),
            (".obsidian/plugins/x/readme.md", "hidden"),
            (".trash/old.md", "trash"),
            ("ArchiVarious/Inbox/capture.md", "inbox"),
            ("ArchiVarious/Notes/kept.md", "kept"),
        ] {
            let file = vault.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let notes = scan_vault(s(&vault), "ArchiVarious/Inbox/".into()).unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "ArchiVarious/Notes/kept.md",
                "Learning/Deep/Loci.md",
                "Learning/Memory.md",
                "Root.MD"
            ]
        );
        assert_eq!(notes[2].text, "# Memory #study");
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn cuts_long_notes_on_a_character_boundary() {
        let vault = temp_vault("long");
        fs::write(vault.join("Long.md"), "я".repeat(5000)).unwrap();
        let notes = scan_vault(s(&vault), "Inbox".into()).unwrap();
        assert!(notes[0].text.starts_with('я'));
        assert!(notes[0].text.len() <= SNIPPET_BYTES as usize + 3);
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn writes_the_section_list_and_section_folders() {
        let vault = temp_vault("sections");
        assert_eq!(read_sections(s(&vault)).unwrap(), None);
        write_sections(
            s(&vault),
            "- Учеба\n  - Математика\n".into(),
            vec!["Учеба/Математика".into()],
        )
        .unwrap();
        assert_eq!(
            read_sections(s(&vault)).unwrap().as_deref(),
            Some("- Учеба\n  - Математика\n")
        );
        assert!(vault.join("Учеба/Математика").is_dir());
        assert!(write_sections(s(&vault), String::new(), vec!["../x".into()]).is_err());
        assert!(write_sections(s(&vault), String::new(), vec![".obsidian".into()]).is_err());
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn reads_the_plugin_data_when_present() {
        let vault = temp_vault("data");
        assert_eq!(read_plugin_data(s(&vault)).unwrap(), None);
        let file = vault.join(PLUGIN_DATA_PATH);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, r#"{"reviewDays":3}"#).unwrap();
        assert_eq!(
            read_plugin_data(s(&vault)).unwrap().as_deref(),
            Some(r#"{"reviewDays":3}"#)
        );
        assert!(read_plugin_data("relative/path".into()).is_err());
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn lists_replaces_and_removes_only_captures() {
        let vault = temp_vault("captures");
        let inbox = vault.join("ArchiVarious/Inbox");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(
            inbox.join("a.md"),
            "---\narchivarious: 1\nid: a\n---\n\nhi\n",
        )
        .unwrap();
        fs::write(inbox.join("note.md"), "# not a capture").unwrap();
        let v = || s(&vault);
        let i = || "ArchiVarious/Inbox".to_string();

        let listed = list_inbox(v(), i()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "a.md");

        assert!(
            replace_inbox_capture(v(), i(), "note.md".into(), "---\narchivarious: 1\n".into())
                .is_err()
        );
        assert!(replace_inbox_capture(v(), i(), "a.md".into(), "plain text".into()).is_err());
        replace_inbox_capture(
            v(),
            i(),
            "a.md".into(),
            "---\narchivarious: 1\nid: b\n---\n".into(),
        )
        .unwrap();
        assert!(fs::read_to_string(inbox.join("a.md"))
            .unwrap()
            .contains("id: b"));

        assert!(remove_inbox_capture(v(), i(), "note.md".into()).is_err());
        assert!(remove_inbox_capture(v(), i(), "a.md".into()).unwrap());
        assert!(!remove_inbox_capture(v(), i(), "a.md".into()).unwrap());
        assert!(inbox.join("note.md").exists());
        assert!(list_inbox(v(), "Missing".into()).unwrap().is_empty());
        fs::remove_dir_all(&vault).unwrap();
    }
}
