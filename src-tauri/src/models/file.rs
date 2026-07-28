use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

/// Maximum text size that we will load into a FileTab (5 MiB).
pub const MAX_TEXT_BYTES: usize = 5 << 20;

/// Image extensions accepted for inline preview (the editor itself is text-only).
pub static IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileContent {
    Text,
    Image,
    Unavailable { reason: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditorState {
    pub selection_offset: Option<usize>,
    pub selection_length: Option<usize>,
    pub scroll_x: Option<f64>,
    pub scroll_y: Option<f64>,
}

/// A file opened as a tab. Text content lives here (not in the view) so
/// edits survive tab switches.
pub struct FileTab {
    pub id: Uuid,
    pub path: Mutex<String>,
    pub content: FileContent,
    pub text: Mutex<String>,
    pub saved_text: Mutex<String>,
    pub editor_state: Mutex<EditorState>,
    pub is_dirty: Mutex<bool>,
}

impl FileTab {
    pub fn open(path: &str) -> Self {
        let lower = PathBuf::from(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if IMAGE_EXTENSIONS.contains(&lower.as_str()) {
            return Self::placeholder(path, FileContent::Image);
        }

        match fs::read(path) {
            Err(e) => Self::placeholder(
                path,
                FileContent::Unavailable { reason: format!("Could not read file: {e}") },
            ),
            Ok(bytes) => {
                if bytes.len() > MAX_TEXT_BYTES {
                    return Self::placeholder(
                        path,
                        FileContent::Unavailable { reason: "File is too large to open".into() },
                    );
                }
                if bytes.contains(&0) {
                    return Self::placeholder(
                        path,
                        FileContent::Unavailable { reason: "Binary file".into() },
                    );
                }
                match String::from_utf8(bytes) {
                    Ok(text) => {
                        let saved = text.clone();
                        Self {
                            id: Uuid::new_v4(),
                            path: Mutex::new(path.to_string()),
                            content: FileContent::Text,
                            text: Mutex::new(text),
                            saved_text: Mutex::new(saved),
                            editor_state: Mutex::new(EditorState::default()),
                            is_dirty: Mutex::new(false),
                        }
                    }
                    Err(_) => Self::placeholder(
                        path,
                        FileContent::Unavailable { reason: "Binary file".into() },
                    ),
                }
            }
        }
    }

    fn placeholder(path: &str, content: FileContent) -> Self {
        Self {
            id: Uuid::new_v4(),
            path: Mutex::new(path.to_string()),
            content,
            text: Mutex::new(String::new()),
            saved_text: Mutex::new(String::new()),
            editor_state: Mutex::new(EditorState::default()),
            is_dirty: Mutex::new(false),
        }
    }

    pub fn name(&self) -> String {
        PathBuf::from(self.path.lock().clone())
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
            .unwrap_or_default()
    }

    pub fn path(&self) -> String { self.path.lock().clone() }

    pub fn update_path(&self, new_path: &str) {
        if new_path != *self.path.lock() {
            *self.path.lock() = new_path.to_string();
        }
    }

    pub fn set_text(&self, text: String) {
        let mut text_lock = self.text.lock();
        if text_lock.as_str() == text.as_str() {
            return;
        }
        *text_lock = text;
        self.refresh_dirty_state();
    }

    pub fn text(&self) -> String { self.text.lock().clone() }

    pub fn refresh_dirty_state(&self) {
        if self.content != FileContent::Text {
            return;
        }
        let dirty = self.text.lock().as_str() != self.saved_text.lock().as_str();
        *self.is_dirty.lock() = dirty;
    }

    pub fn save(&self) -> Result<(), String> {
        if self.content != FileContent::Text {
            return Ok(());
        }
        let bytes = {
            let text = self.text.lock();
            text.clone().into_bytes()
        };
        fs::write(self.path(), bytes).map_err(|e| e.to_string())?;
        let saved = self.text.lock().clone();
        *self.saved_text.lock() = saved;
        *self.is_dirty.lock() = false;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTabInfo {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub content_kind: String,
    pub text: String,
    pub is_dirty: bool,
}

impl From<&FileTab> for FileTabInfo {
    fn from(f: &FileTab) -> Self {
        let content_kind = match f.content {
            FileContent::Text => "text",
            FileContent::Image => "image",
            FileContent::Unavailable { .. } => "unavailable",
        };
        let text = if matches!(f.content, FileContent::Text) { f.text() } else { String::new() };
        Self {
            id: f.id,
            path: f.path(),
            name: f.name(),
            content_kind: content_kind.into(),
            text,
            is_dirty: *f.is_dirty.lock(),
        }
    }
}