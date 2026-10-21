//! Typed payloads for the events the WebView subscribes to.
//!
//! The backend still emits these with `json!`/`emit_to`; the structs are the
//! single source for the generated `events` bindings, so any payload change
//! shows up as a TypeScript error in the listeners.

use serde::Serialize;
use uuid::Uuid;

use crate::models::app::AppStateView;
use crate::services::agents::{AgentAttention, AgentKind, AgentState};

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct StateChanged(pub AppStateView);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(transparent)]
pub struct TabFocused(pub Uuid);

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct FsChanged {
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct FileSaved {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct FileReveal {
    pub id: Uuid,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "pty:data")]
pub struct PtyData {
    pub id: Uuid,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "pty:exit")]
pub struct PtyExit {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "pty:progress")]
pub struct PtyProgress {
    pub id: Uuid,
    pub state: u8,
    pub progress: u8,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct SessionCwdChanged {
    pub id: Uuid,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AgentStatusRow {
    pub id: Uuid,
    pub agent: Option<AgentKind>,
    pub state: Option<AgentState>,
    pub attention: Option<AgentAttention>,
    pub title: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct AgentStatusChanged {
    pub sessions: Vec<AgentStatusRow>,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct AllAgentStatus {
    pub sessions: Vec<AgentStatusRow>,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct NotificationActivate {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct OpenSettings;

#[derive(Debug, Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct SettingsChanged;
