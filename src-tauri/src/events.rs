//! Tauri event names and the queue event bridge.

use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::queue::{EventSink, Job};

pub const QUEUE_JOB: &str = "queue://job";
pub const QUEUE_CHANGED: &str = "queue://changed";
pub const DEPS_PROGRESS: &str = "deps://progress";
pub const DEPS_CHANGED: &str = "deps://changed";
pub const SETTINGS_CHANGED: &str = "settings://changed";
pub const UPDATER_PROGRESS: &str = "updater://progress";
pub const HISTORY_CHANGED: &str = "history://changed";

pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn job_updated(&self, job: &Job) {
        let _ = self.app.emit(QUEUE_JOB, job);
        if job.state.is_finished() {
            let _ = self.app.emit(HISTORY_CHANGED, ());
        }
    }

    fn queue_changed(&self, jobs: &[Job]) {
        let _ = self.app.emit(QUEUE_CHANGED, jobs);
    }

    fn notify(&self, title: &str, body: &str) {
        if let Err(e) = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            tracing::warn!("desktop notification failed: {e}");
        }
    }
}
