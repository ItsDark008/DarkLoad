//! Asynchronous download queue.
//!
//! The queue owns every download job. A scheduler task starts jobs in queue order up to the
//! configured concurrency; each worker drives one yt-dlp process, translates its structured
//! output into progress, places the finished files and records history. All state changes are
//! persisted so pending work survives application restarts.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::converter;
use crate::dependencies::DependencyManager;
use crate::download::{self, DownloadSpec, Event, ProgressTracker};
use crate::errors::{AppError, AppResult, ErrorCode};
use crate::extractor::{self, Analysis, MediaInfo};
use crate::formats::{self, DownloadOptions, SelectionPlan};
use crate::platform::{self, Platform};
use crate::process;
use crate::security::{self, DuplicatePolicy, Placement};
use crate::settings::{MediaType, PartialFilePolicy, Settings};
use crate::storage::{HistoryEntry, Storage};

pub const PROGRESS_FILE: &str = ".darkload-ffmpeg-progress.txt";
const EMIT_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Analyzing,
    Queued,
    Downloading,
    Merging,
    Converting,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_running(self) -> bool {
        matches!(
            self,
            JobState::Analyzing | JobState::Downloading | JobState::Merging | JobState::Converting
        )
    }

    pub fn is_finished(self) -> bool {
        matches!(
            self,
            JobState::Completed | JobState::Failed | JobState::Cancelled
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Analyzing => "analyzing",
            JobState::Queued => "queued",
            JobState::Downloading => "downloading",
            JobState::Merging => "merging",
            JobState::Converting => "converting",
            JobState::Completed => "completed",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
        }
    }
}

/// A download submitted by the interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub options: DownloadOptions,
    #[serde(default)]
    pub playlist_item: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub uploader: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub thumbnail_local: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobError {
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<String>,
    pub retryable: bool,
}

impl From<&AppError> for JobError {
    fn from(e: &AppError) -> Self {
        Self {
            code: e.code,
            message: e.message.clone(),
            detail: e.detail.clone(),
            retryable: e.retryable(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    /// Download completion (0–1) across all streams, when known.
    pub download_fraction: Option<f64>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    /// Current post-processing step, for example "Merging streams".
    pub process_label: Option<String>,
    /// Merge/conversion completion (0–1), measured from FFmpeg output.
    pub process_fraction: Option<f64>,
    pub process_eta: Option<f64>,
    /// Retry status shown while waiting between automatic attempts.
    pub retry_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub request: DownloadRequest,
    pub platform: Platform,
    pub state: JobState,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub thumbnail_local: Option<String>,
    pub duration: Option<f64>,
    pub output_dir: String,
    pub plan: Option<SelectionPlan>,
    pub progress: JobProgress,
    pub error: Option<JobError>,
    pub notes: Vec<String>,
    pub attempts: u32,
    pub paused: bool,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub output_path: Option<String>,
    pub file_size: Option<u64>,
}

/// Delivers queue updates to the interface.
pub trait EventSink: Send + Sync + 'static {
    fn job_updated(&self, job: &Job);
    fn queue_changed(&self, jobs: &[Job]);
    fn notify(&self, title: &str, body: &str);
}

/// Recently analyzed media, reused when the user enqueues what they just analyzed.
#[derive(Default)]
pub struct AnalysisCache {
    inner: Mutex<HashMap<String, (Instant, Arc<MediaInfo>)>>,
}

impl AnalysisCache {
    const TTL: Duration = Duration::from_secs(4 * 3600);
    const CAPACITY: usize = 200;

    fn key(url: &str, item: Option<u32>) -> String {
        format!("{url}#{}", item.unwrap_or(0))
    }

    pub fn put(&self, url: &str, item: Option<u32>, media: MediaInfo) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= Self::CAPACITY {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (t, _))| *t)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest);
            }
        }
        map.insert(Self::key(url, item), (Instant::now(), Arc::new(media)));
    }

    pub fn get(&self, url: &str, item: Option<u32>) -> Option<Arc<MediaInfo>> {
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&Self::key(url, item))
            .filter(|(t, _)| t.elapsed() < Self::TTL)
            .map(|(_, m)| m.clone())
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

pub struct QueuePaths {
    pub jobs_dir: PathBuf,
    pub ytdlp_cache: PathBuf,
    pub thumbnails_dir: PathBuf,
}

#[derive(Default)]
struct Inner {
    jobs: Vec<Job>,
    tokens: HashMap<String, CancellationToken>,
    last_emit: HashMap<String, Instant>,
}

pub struct QueueManager {
    inner: Mutex<Inner>,
    notify: Notify,
    storage: Arc<Storage>,
    deps: Arc<DependencyManager>,
    settings: Arc<RwLock<Settings>>,
    cache: Arc<AnalysisCache>,
    paths: QueuePaths,
    sink: Arc<dyn EventSink>,
    shutdown: CancellationToken,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

enum Outcome {
    Placed { path: PathBuf, size: Option<u64> },
    Skipped { path: PathBuf, size: Option<u64> },
}

impl QueueManager {
    pub fn new(
        storage: Arc<Storage>,
        deps: Arc<DependencyManager>,
        settings: Arc<RwLock<Settings>>,
        cache: Arc<AnalysisCache>,
        paths: QueuePaths,
        sink: Arc<dyn EventSink>,
    ) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&paths.jobs_dir);
        let _ = std::fs::create_dir_all(&paths.ytdlp_cache);
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            notify: Notify::new(),
            storage,
            deps,
            settings,
            cache,
            paths,
            sink,
            shutdown: CancellationToken::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Restores persisted jobs and starts the scheduler.
    pub fn start(self: &Arc<Self>) {
        let resume = self.settings().resume_queue_on_startup;
        match self.storage.load_queue() {
            Ok(rows) => {
                let mut inner = self.lock();
                for row in rows {
                    match serde_json::from_str::<Job>(&row) {
                        Ok(mut job) => {
                            if job.state.is_running() || job.state == JobState::Queued {
                                job.state = if resume && !job.paused {
                                    JobState::Queued
                                } else {
                                    JobState::Pending
                                };
                                job.progress = JobProgress::default();
                                if !resume {
                                    job.paused = true;
                                }
                            }
                            inner.jobs.push(job);
                        }
                        Err(e) => tracing::warn!("dropping unreadable queue entry: {e}"),
                    }
                }
            }
            Err(e) => tracing::error!("failed to restore the download queue: {}", e.message),
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                this.schedule();
                tokio::select! {
                    _ = this.notify.notified() => {}
                    _ = this.shutdown.cancelled() => break,
                }
            }
        });
        self.notify.notify_one();
    }

    pub fn wake(&self) {
        self.notify.notify_one();
    }

    fn persist(&self) {
        let rows: Vec<(String, String)> = {
            let inner = self.lock();
            inner
                .jobs
                .iter()
                .filter_map(|j| {
                    serde_json::to_string(j)
                        .ok()
                        .map(|json| (j.id.clone(), json))
                })
                .collect()
        };
        if let Err(e) = self.storage.save_queue(&rows) {
            tracing::error!("failed to persist queue: {}", e.message);
        }
    }

    fn emit_snapshot(&self) {
        let jobs = self.lock().jobs.clone();
        self.sink.queue_changed(&jobs);
    }

    /// Applies `f` to a job and emits the change. Progress-only updates are throttled.
    fn update<F: FnOnce(&mut Job)>(&self, id: &str, structural: bool, f: F) -> Option<Job> {
        let snapshot = {
            let mut inner = self.lock();
            let job = inner.jobs.iter_mut().find(|j| j.id == id)?;
            let before = job.state;
            f(job);
            let state_changed = job.state != before;
            let job = job.clone();
            let due = inner
                .last_emit
                .get(id)
                .is_none_or(|t| t.elapsed() >= EMIT_INTERVAL);
            if !(structural || state_changed || due) {
                return Some(job);
            }
            inner.last_emit.insert(id.to_string(), Instant::now());
            (job, structural || state_changed)
        };
        self.sink.job_updated(&snapshot.0);
        if snapshot.1 {
            self.persist();
        }
        Some(snapshot.0)
    }

    pub fn list(&self) -> Vec<Job> {
        self.lock().jobs.clone()
    }

    pub fn active_count(&self) -> usize {
        self.lock()
            .jobs
            .iter()
            .filter(|j| j.state.is_running() || j.state == JobState::Queued)
            .count()
    }

    pub fn running_count(&self) -> usize {
        self.lock()
            .jobs
            .iter()
            .filter(|j| j.state.is_running())
            .count()
    }

    /// Validates and adds downloads to the end of the queue.
    pub fn enqueue(&self, requests: Vec<DownloadRequest>) -> AppResult<Vec<Job>> {
        if requests.is_empty() {
            return Err(AppError::invalid_input("Nothing to add to the queue."));
        }
        if requests.len() > 5000 {
            return Err(AppError::invalid_input("Add at most 5000 items at a time."));
        }
        let settings = self.settings();
        let mut jobs = Vec::with_capacity(requests.len());
        for mut request in requests {
            let detected = platform::detect(&request.url)?;
            request.url = detected.url.clone();
            request.options.validate()?;
            let dir = request
                .output_dir
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| settings.download_dir.clone());
            let output_dir = security::validate_output_dir(&dir)?;
            let mut job = Job {
                id: uuid::Uuid::new_v4().to_string(),
                platform: detected.platform,
                state: JobState::Queued,
                title: request
                    .title
                    .clone()
                    .unwrap_or_else(|| detected.url.clone()),
                uploader: request.uploader.clone(),
                thumbnail: request.thumbnail.clone(),
                thumbnail_local: request.thumbnail_local.clone(),
                duration: request.duration,
                output_dir: output_dir.to_string_lossy().into_owned(),
                plan: None,
                progress: JobProgress::default(),
                error: None,
                notes: Vec::new(),
                attempts: 0,
                paused: false,
                created_at: now_ms(),
                started_at: None,
                finished_at: None,
                output_path: None,
                file_size: None,
                request,
            };
            if let Some(media) = self.cache.get(&job.request.url, job.request.playlist_item) {
                apply_media(&mut job, &media);
                let plan = formats::plan(&media.formats, media.duration, &job.request.options)?;
                apply_plan_notes(&mut job, &plan);
                job.plan = Some(plan);
            }
            jobs.push(job);
        }
        {
            let mut inner = self.lock();
            inner.jobs.extend(jobs.iter().cloned());
        }
        self.persist();
        self.emit_snapshot();
        self.wake();
        Ok(jobs)
    }

    fn schedule(self: &Arc<Self>) {
        let max = usize::from(self.settings().max_concurrent_downloads.max(1));
        let mut to_start = Vec::new();
        {
            let mut inner = self.lock();
            let running = inner.jobs.iter().filter(|j| j.state.is_running()).count();
            let slots = max.saturating_sub(running);
            let ids: Vec<String> = inner
                .jobs
                .iter()
                .filter(|j| j.state == JobState::Queued)
                .take(slots)
                .map(|j| j.id.clone())
                .collect();
            for id in ids {
                let token = self.shutdown.child_token();
                inner.tokens.insert(id.clone(), token.clone());
                if let Some(job) = inner.jobs.iter_mut().find(|j| j.id == id) {
                    job.state = if job.plan.is_some() {
                        JobState::Downloading
                    } else {
                        JobState::Analyzing
                    };
                    job.started_at = Some(now_ms());
                    job.finished_at = None;
                    job.error = None;
                    job.progress = JobProgress::default();
                    to_start.push((job.clone(), token));
                }
            }
        }
        if to_start.is_empty() {
            return;
        }
        self.persist();
        for (job, token) in to_start {
            self.sink.job_updated(&job);
            let this = self.clone();
            tauri::async_runtime::spawn(async move {
                this.run_job(job.id, token).await;
            });
        }
    }

    async fn run_job(self: Arc<Self>, id: String, token: CancellationToken) {
        let result = self.execute(&id, &token).await;
        if self.shutdown.is_cancelled() {
            // Application exit: keep the persisted state so the job resumes on next launch.
            return;
        }
        let settings = self.settings();
        let work_dir = self.paths.jobs_dir.join(&id);
        let mut notify: Option<(String, String)> = None;
        let job = self.update(&id, true, |job| {
            job.progress.speed = None;
            job.progress.eta = None;
            job.progress.retry_label = None;
            job.finished_at = Some(now_ms());
            match &result {
                Ok(Outcome::Placed { path, size }) | Ok(Outcome::Skipped { path, size }) => {
                    if matches!(result, Ok(Outcome::Skipped { .. })) {
                        job.notes.push("A file with the same name already exists, so the new download was not saved.".into());
                    }
                    job.state = JobState::Completed;
                    job.output_path = Some(path.to_string_lossy().into_owned());
                    job.file_size = *size;
                    job.progress.download_fraction = Some(1.0);
                    job.progress.process_fraction = job.progress.process_label.as_ref().map(|_| 1.0);
                    notify = Some(("Download complete".into(), job.title.clone()));
                }
                Err(e) if e.code == ErrorCode::Cancelled && job.paused => {
                    job.state = JobState::Pending;
                    job.finished_at = None;
                }
                Err(e) if e.code == ErrorCode::Cancelled => job.state = JobState::Cancelled,
                Err(e) => {
                    job.state = JobState::Failed;
                    job.error = Some(JobError::from(e));
                    notify = Some(("Download failed".into(), format!("{} — {}", job.title, e.message)));
                }
            }
        });
        {
            let mut inner = self.lock();
            inner.tokens.remove(&id);
            inner.last_emit.remove(&id);
        }
        if let Some(job) = job {
            let keep_partial = job.state == JobState::Pending
                || settings.partial_files == PartialFilePolicy::Keep
                    && job.state != JobState::Completed;
            if !keep_partial {
                let _ = std::fs::remove_dir_all(&work_dir);
            }
            if job.state.is_finished() {
                self.record_history(&job);
                if settings.notifications {
                    if let Some((title, body)) = notify {
                        self.sink.notify(&title, &body);
                    }
                }
            }
        }
        self.wake();
    }

    fn record_history(&self, job: &Job) {
        let format = match job.request.options.media_type {
            MediaType::Video => job
                .plan
                .as_ref()
                .and_then(|p| p.output_container)
                .and_then(|c| c.ext())
                .map(str::to_string),
            MediaType::Audio => job
                .output_path
                .as_deref()
                .and_then(|p| Path::new(p).extension())
                .map(|e| e.to_string_lossy().into_owned()),
        }
        .or_else(|| {
            job.output_path
                .as_deref()
                .and_then(|p| Path::new(p).extension())
                .map(|e| e.to_string_lossy().into_owned())
        });
        let entry = HistoryEntry {
            id: job.id.clone(),
            url: job.request.url.clone(),
            title: job.title.clone(),
            uploader: job.uploader.clone(),
            duration: job.duration,
            thumbnail: job.thumbnail.clone(),
            thumbnail_local: job.thumbnail_local.clone(),
            platform: job.platform.as_str().into(),
            media_type: match job.request.options.media_type {
                MediaType::Video => "video".into(),
                MediaType::Audio => "audio".into(),
            },
            format,
            quality: job
                .plan
                .as_ref()
                .map(|p| p.quality_label.clone())
                .filter(|q| !q.is_empty()),
            file_size: job.file_size,
            file_path: job.output_path.clone(),
            status: job.state.as_str().into(),
            error: job.error.as_ref().map(|e| e.message.clone()),
            created_at: job.started_at.unwrap_or(job.created_at),
            completed_at: job.finished_at,
            file_exists: false,
            request_json: serde_json::to_string(&job.request).unwrap_or_else(|_| "{}".into()),
        };
        if let Err(e) = self.storage.upsert_history(&entry) {
            tracing::error!("failed to record history: {}", e.message);
        }
    }

    async fn execute(&self, id: &str, token: &CancellationToken) -> AppResult<Outcome> {
        let job = self
            .lock()
            .jobs
            .iter()
            .find(|j| j.id == id)
            .cloned()
            .ok_or_else(|| AppError::internal("The job no longer exists."))?;
        let settings = self.settings();
        let tools = self.deps.tool_paths(self.paths.ytdlp_cache.clone()).await?;

        // Resolve formats when the job was added without a fresh analysis.
        let plan = match job.plan.clone() {
            Some(plan) => plan,
            None => {
                let detected = platform::detect(&job.request.url)?;
                let analysis =
                    extractor::analyze(&tools, &detected, (&settings).into(), token).await?;
                let media = match analysis {
                    Analysis::Media(m) => *m,
                    Analysis::Playlist(p) => match job.request.playlist_item {
                        Some(item) => p
                            .entries
                            .into_iter()
                            .find(|e| e.playlist_item == Some(item))
                            .and_then(|e| e.media)
                            .map(|m| *m)
                            .ok_or_else(|| AppError::new(ErrorCode::Unavailable, "This item is no longer part of the post."))?,
                        None => return Err(AppError::invalid_input("This link is a playlist. Add its videos from the Downloader or Batch page.")),
                    },
                };
                let mut media = media;
                if media.thumbnail_local.is_none() {
                    if let Some(url) = media.thumbnail.clone() {
                        media.thumbnail_local = crate::thumbnails::cache(
                            self.deps.http(),
                            &self.paths.thumbnails_dir,
                            &url,
                        )
                        .await
                        .map(|p| p.to_string_lossy().into_owned());
                    }
                }
                let plan = formats::plan(&media.formats, media.duration, &job.request.options)?;
                self.update(id, true, |job| {
                    apply_media(job, &media);
                    apply_plan_notes(job, &plan);
                    job.plan = Some(plan.clone());
                    job.state = JobState::Downloading;
                });
                self.cache
                    .put(&job.request.url, job.request.playlist_item, media);
                plan
            }
        };

        let needs_ffmpeg = plan.requires_merge
            || job.request.options.media_type == MediaType::Audio
            || job.request.options.embed_metadata
            || plan.embed_thumbnail
            || plan.output_container.is_some();
        if needs_ffmpeg && tools.ffmpeg.is_none() {
            return Err(AppError::new(
                ErrorCode::DependencyMissing,
                "FFmpeg is required for this download. Install it from Settings → Dependencies.",
            ));
        }

        let output_dir = security::validate_output_dir(&job.output_dir)?;
        let work_dir = self.paths.jobs_dir.join(id);
        std::fs::create_dir_all(&work_dir)?;
        check_disk_space(&plan, &work_dir, &output_dir)?;

        let progress_file = work_dir.join(PROGRESS_FILE);
        let stream_sizes: Vec<Option<u64>> = [plan.video.as_ref(), plan.audio.as_ref()]
            .into_iter()
            .flatten()
            .map(|f| f.size().map(|s| s.0))
            .collect();
        let expected_streams = stream_sizes.len().max(1);

        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let spec = DownloadSpec {
                url: &job.request.url,
                playlist_item: job.request.playlist_item,
                options: &job.request.options,
                plan: &plan,
                settings: &settings,
                work_dir: &work_dir,
                ffmpeg_progress_file: Some(&progress_file),
            };
            let args: Vec<OsString> = download::build_args(&tools, &spec);
            self.update(id, true, |job| {
                job.attempts = attempt;
                job.progress.retry_label = None;
                if job.state == JobState::Analyzing || job.state.is_running() {
                    job.state = JobState::Downloading;
                }
            });
            let result = self
                .run_download(
                    id,
                    &tools.yt_dlp,
                    &args,
                    &work_dir,
                    &progress_file,
                    token,
                    expected_streams,
                    stream_sizes.clone(),
                )
                .await;
            match result {
                Ok(final_path) => {
                    let _ = std::fs::remove_file(&progress_file);
                    return place_outputs(
                        &work_dir,
                        &final_path,
                        &output_dir,
                        settings.duplicate_policy,
                    );
                }
                Err(e) if e.code == ErrorCode::Cancelled => return Err(e),
                Err(e) if e.retryable() && attempt <= u32::from(settings.retry_attempts) => {
                    let delay = Duration::from_secs(2u64.pow(attempt.min(5)));
                    tracing::warn!(
                        "download {id} failed (attempt {attempt}): {}; retrying in {delay:?}",
                        e.message
                    );
                    let max = settings.retry_attempts;
                    self.update(id, true, |job| {
                        job.progress.retry_label =
                            Some(format!("{} Retrying ({}/{max})…", e.message, attempt));
                        job.progress.speed = None;
                        job.progress.eta = None;
                    });
                    tokio::select! {
                        _ = token.cancelled() => return Err(AppError::cancelled()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_download(
        &self,
        id: &str,
        yt_dlp: &Path,
        args: &[OsString],
        work_dir: &Path,
        progress_file: &Path,
        token: &CancellationToken,
        expected_streams: usize,
        stream_sizes: Vec<Option<u64>>,
    ) -> AppResult<PathBuf> {
        let duration = self
            .lock()
            .jobs
            .iter()
            .find(|j| j.id == id)
            .and_then(|j| j.duration);
        let ticker_token = token.child_token();
        // Polls FFmpeg's progress file while merging or converting. It runs concurrently with
        // the process future below and stops as soon as the process exits.
        let ticker = async {
            let mut interval = tokio::time::interval(Duration::from_millis(400));
            loop {
                tokio::select! {
                    _ = ticker_token.cancelled() => break,
                    _ = interval.tick() => {
                        let state = self.lock().jobs.iter().find(|j| j.id == id).map(|j| j.state);
                        if matches!(state, Some(JobState::Merging | JobState::Converting)) {
                            if let Some(p) = converter::read_progress_file(progress_file) {
                                self.update(id, false, |job| {
                                    job.progress.process_fraction = converter::fraction(&p, duration);
                                    job.progress.process_eta = converter::eta(&p, duration);
                                });
                            }
                        }
                    }
                }
            }
        };

        let mut tracker = ProgressTracker::new(expected_streams, stream_sizes);
        let mut final_path: Option<PathBuf> = None;
        let run = async {
            let r = process::run(yt_dlp, args, Some(work_dir), token, None, false, |line| {
                match download::parse_line(&line.text) {
                    Some(Event::Progress {
                        format_id,
                        progress,
                    }) => {
                        let agg = tracker.update(format_id.as_deref(), &progress);
                        self.update(id, false, |job| {
                            if !matches!(job.state, JobState::Merging | JobState::Converting) {
                                job.state = JobState::Downloading;
                            }
                            job.progress.download_fraction =
                                agg.fraction.or(job.progress.download_fraction);
                            job.progress.downloaded_bytes = agg.downloaded;
                            job.progress.total_bytes = agg.total;
                            job.progress.speed = agg.speed;
                            job.progress.eta = agg.eta;
                        });
                    }
                    Some(Event::PostProcess {
                        status,
                        postprocessor,
                    }) => {
                        if download::runs_before_download(&postprocessor) {
                            // Thumbnail and subtitle conversion happen before the media
                            // download starts; they are not a processing stage.
                        } else if status == "started" {
                            let _ = std::fs::remove_file(progress_file);
                            self.update(id, false, |job| {
                                job.state = if download::is_merge(&postprocessor) {
                                    JobState::Merging
                                } else {
                                    JobState::Converting
                                };
                                job.progress.download_fraction = Some(1.0);
                                job.progress.speed = None;
                                job.progress.eta = None;
                                job.progress.process_label =
                                    Some(download::postprocessor_label(&postprocessor).into());
                                job.progress.process_fraction = Some(0.0);
                                job.progress.process_eta = None;
                            });
                        } else if status == "finished" {
                            self.update(id, false, |job| job.progress.process_fraction = Some(1.0));
                        }
                    }
                    Some(Event::File(path)) => final_path = Some(PathBuf::from(path)),
                    None => {
                        if line.stream == process::Stream::Stderr {
                            tracing::debug!(target: "yt-dlp", "{}", line.text);
                        }
                    }
                }
            })
            .await;
            ticker_token.cancel();
            r
        };
        let (finished, ()) = tokio::join!(run, ticker);
        let finished = finished?;
        if !finished.success {
            return Err(extractor::classify_error(&finished.stderr_tail));
        }
        let path = final_path.ok_or_else(|| {
            AppError::new(
                ErrorCode::ProcessFailed,
                "The download finished without producing a file.",
            )
        })?;
        if !path.is_file() {
            return Err(AppError::new(
                ErrorCode::ProcessFailed,
                "The downloaded file could not be found.",
            ));
        }
        Ok(path)
    }

    fn token(&self, id: &str) -> Option<CancellationToken> {
        self.lock().tokens.get(id).cloned()
    }

    pub fn cancel(&self, id: &str) -> AppResult<()> {
        let state = self
            .lock()
            .jobs
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.state)
            .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
        if state.is_running() {
            self.update(id, true, |job| job.paused = false);
            if let Some(t) = self.token(id) {
                t.cancel();
            }
        } else if matches!(state, JobState::Queued | JobState::Pending) {
            self.update(id, true, |job| {
                job.state = JobState::Cancelled;
                job.paused = false;
                job.finished_at = Some(now_ms());
            });
            let _ = std::fs::remove_dir_all(self.paths.jobs_dir.join(id));
        }
        Ok(())
    }

    /// Stops a download but keeps partial data so it can resume later.
    pub fn pause(&self, id: &str) -> AppResult<()> {
        let state = self
            .lock()
            .jobs
            .iter()
            .find(|j| j.id == id)
            .map(|j| j.state)
            .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
        match state {
            s if s.is_running() => {
                self.update(id, true, |job| job.paused = true);
                if let Some(t) = self.token(id) {
                    t.cancel();
                }
            }
            JobState::Queued => {
                self.update(id, true, |job| {
                    job.paused = true;
                    job.state = JobState::Pending;
                });
            }
            _ => {}
        }
        Ok(())
    }

    pub fn resume(&self, id: &str) -> AppResult<()> {
        self.update(id, true, |job| {
            if job.state == JobState::Pending {
                job.paused = false;
                job.state = JobState::Queued;
            }
        })
        .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
        self.wake();
        Ok(())
    }

    pub fn retry(&self, id: &str) -> AppResult<()> {
        self.update(id, true, |job| {
            if matches!(job.state, JobState::Failed | JobState::Cancelled) {
                // Re-resolve formats when the previous attempt reported them unavailable.
                if job
                    .error
                    .as_ref()
                    .is_some_and(|e| e.code == ErrorCode::FormatUnavailable)
                {
                    job.plan = None;
                }
                job.state = JobState::Queued;
                job.paused = false;
                job.error = None;
                job.attempts = 0;
                job.finished_at = None;
                job.progress = JobProgress::default();
            }
        })
        .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
        self.wake();
        Ok(())
    }

    pub fn remove(&self, ids: &[String]) -> AppResult<()> {
        {
            let mut inner = self.lock();
            if ids.iter().any(|id| {
                inner
                    .jobs
                    .iter()
                    .any(|j| &j.id == id && j.state.is_running())
            }) {
                return Err(AppError::invalid_input(
                    "Cancel running downloads before removing them.",
                ));
            }
            inner.jobs.retain(|j| !ids.contains(&j.id));
        }
        for id in ids {
            let _ = std::fs::remove_dir_all(self.paths.jobs_dir.join(id));
        }
        self.persist();
        self.emit_snapshot();
        Ok(())
    }

    pub fn clear_finished(&self) -> usize {
        let removed: Vec<String> = {
            let mut inner = self.lock();
            let ids = inner
                .jobs
                .iter()
                .filter(|j| j.state.is_finished())
                .map(|j| j.id.clone())
                .collect();
            inner.jobs.retain(|j| !j.state.is_finished());
            ids
        };
        for id in &removed {
            let _ = std::fs::remove_dir_all(self.paths.jobs_dir.join(id));
        }
        self.persist();
        self.emit_snapshot();
        removed.len()
    }

    /// Moves a job to `index` in the queue order.
    pub fn move_to(&self, id: &str, index: usize) -> AppResult<()> {
        {
            let mut inner = self.lock();
            let from = inner
                .jobs
                .iter()
                .position(|j| j.id == id)
                .ok_or_else(|| AppError::invalid_input("Unknown download."))?;
            let job = inner.jobs.remove(from);
            let to = index.min(inner.jobs.len());
            inner.jobs.insert(to, job);
        }
        self.persist();
        self.emit_snapshot();
        Ok(())
    }

    pub fn pause_all(&self) {
        let ids: Vec<String> = self
            .lock()
            .jobs
            .iter()
            .filter(|j| j.state.is_running() || j.state == JobState::Queued)
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            let _ = self.pause(&id);
        }
    }

    pub fn resume_all(&self) {
        let ids: Vec<String> = self
            .lock()
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Pending)
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            let _ = self.resume(&id);
        }
    }

    /// Stops all workers and their child processes (used on application exit).
    pub async fn shutdown(&self) {
        {
            let mut inner = self.lock();
            for job in inner.jobs.iter_mut().filter(|j| j.state.is_running()) {
                job.paused = !self.settings().resume_queue_on_startup;
            }
        }
        self.persist();
        self.shutdown.cancel();
        // Give workers a moment to terminate their process trees.
        for _ in 0..50 {
            if self.running_count() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Deletes leftover work folders that no queued job references.
    pub fn clear_temporary_files(&self) -> AppResult<u64> {
        let keep: Vec<String> = self
            .lock()
            .jobs
            .iter()
            .filter(|j| !j.state.is_finished())
            .map(|j| j.id.clone())
            .collect();
        let mut freed = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.paths.jobs_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if keep.contains(&name) {
                    continue;
                }
                freed += dir_size(&entry.path());
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        Ok(freed)
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&entry.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

fn apply_media(job: &mut Job, media: &MediaInfo) {
    job.title = media.title.clone();
    job.uploader = media.uploader.clone().or(job.uploader.take());
    job.thumbnail = media.thumbnail.clone().or(job.thumbnail.take());
    job.thumbnail_local = media.thumbnail_local.clone().or(job.thumbnail_local.take());
    job.duration = media.duration.or(job.duration);
}

fn apply_plan_notes(job: &mut Job, plan: &SelectionPlan) {
    if let Some(conflict) = &plan.container_conflict {
        job.notes.push(format!(
            "Saved as {}: {}",
            conflict.suggested.label(),
            conflict.reason
        ));
    }
    for note in plan
        .notes
        .iter()
        .filter(|n| n.level == formats::NoteLevel::Warning)
    {
        job.notes.push(note.message.clone());
    }
}

fn check_disk_space(plan: &SelectionPlan, work_dir: &Path, output_dir: &Path) -> AppResult<()> {
    let Some(size) = plan.estimated_size else {
        return Ok(());
    };
    // Merging temporarily needs the streams and the merged output side by side.
    let factor = if plan.requires_merge || plan.transcodes_audio {
        2.2
    } else {
        1.2
    };
    let needed = (size as f64 * factor) as u64 + 50 * 1024 * 1024;
    for dir in [work_dir, output_dir] {
        if let Some(free) = security::available_space(dir) {
            if free < needed {
                return Err(AppError::new(
                    ErrorCode::DiskFull,
                    format!(
                        "Not enough free disk space: about {} MB is needed and {} MB is available.",
                        needed / 1_048_576,
                        free / 1_048_576
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn is_temporary(name: &str) -> bool {
    name == PROGRESS_FILE
        || name.ends_with(".part")
        || name.ends_with(".ytdl")
        || name.contains(".part-Frag")
        || name.contains(".temp.")
}

/// Moves the finished file (and side files such as subtitles) into the output folder.
fn place_outputs(
    work_dir: &Path,
    final_path: &Path,
    output_dir: &Path,
    policy: DuplicatePolicy,
) -> AppResult<Outcome> {
    if !security::is_within(work_dir, final_path) {
        return Err(AppError::new(
            ErrorCode::InvalidPath,
            "The downloaded file was written outside the working folder.",
        ));
    }
    let main_name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::internal("Invalid output file name."))?;
    let outcome = match security::resolve_destination(output_dir, &main_name, policy) {
        Placement::Placed(target) => {
            security::move_file(final_path, &target, policy == DuplicatePolicy::Overwrite)?;
            let size = std::fs::metadata(&target).ok().map(|m| m.len());
            Outcome::Placed { path: target, size }
        }
        Placement::SkippedExisting(existing) => {
            let size = std::fs::metadata(&existing).ok().map(|m| m.len());
            Outcome::Skipped {
                path: existing,
                size,
            }
        }
    };
    if matches!(outcome, Outcome::Placed { .. }) {
        for entry in std::fs::read_dir(work_dir)?.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !path.is_file() || is_temporary(&name) {
                continue;
            }
            if let Placement::Placed(target) =
                security::resolve_destination(output_dir, &name, policy)
            {
                let _ = security::move_file(&path, &target, policy == DuplicatePolicy::Overwrite);
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::DepId;

    #[derive(Default)]
    struct TestSink {
        updates: Mutex<Vec<Job>>,
        notifications: Mutex<Vec<String>>,
    }

    impl EventSink for TestSink {
        fn job_updated(&self, job: &Job) {
            self.updates.lock().unwrap().push(job.clone());
        }
        fn queue_changed(&self, _jobs: &[Job]) {}
        fn notify(&self, title: &str, _body: &str) {
            self.notifications.lock().unwrap().push(title.to_string());
        }
    }

    struct Harness {
        _dir: tempfile::TempDir,
        out: PathBuf,
        jobs_dir: PathBuf,
        queue: Arc<QueueManager>,
        storage: Arc<Storage>,
        sink: Arc<TestSink>,
        settings: Arc<RwLock<Settings>>,
    }

    fn fake_tool() -> Option<PathBuf> {
        // `cargo test` builds examples; the fake extractor lives next to the test binary.
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.parent()?.join("examples");
        let path = dir.join(format!("fake_ytdlp{}", std::env::consts::EXE_SUFFIX));
        path.is_file().then_some(path)
    }

    async fn harness(install_tools: bool, configure: impl FnOnce(&mut Settings)) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Downloads Ünïcode");
        std::fs::create_dir_all(&out).unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        if install_tools {
            let fake =
                fake_tool().expect("fake_ytdlp example must be built (cargo test builds examples)");
            for id in [DepId::YtDlp, DepId::Ffmpeg, DepId::Ffprobe] {
                std::fs::copy(&fake, bin.join(id.exe_name())).unwrap();
            }
        }
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let deps = Arc::new(DependencyManager::new(bin, false).unwrap());
        deps.refresh().await;
        let mut s = Settings {
            download_dir: out.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        configure(&mut s);
        let settings = Arc::new(RwLock::new(s));
        let sink = Arc::new(TestSink::default());
        let jobs_dir = dir.path().join("jobs");
        let queue = QueueManager::new(
            storage.clone(),
            deps,
            settings.clone(),
            Arc::new(AnalysisCache::default()),
            QueuePaths {
                jobs_dir: jobs_dir.clone(),
                ytdlp_cache: dir.path().join("cache"),
                thumbnails_dir: dir.path().join("thumbs"),
            },
            sink.clone(),
        );
        Harness {
            _dir: dir,
            out,
            jobs_dir,
            queue,
            storage,
            sink,
            settings,
        }
    }

    fn request(id: &str) -> DownloadRequest {
        DownloadRequest {
            url: format!("https://www.youtube.com/watch?v={id}"),
            output_dir: None,
            options: DownloadOptions::default(),
            playlist_item: None,
            title: Some(format!("Title {id}")),
            uploader: None,
            thumbnail: None,
            thumbnail_local: None,
            duration: Some(10.0),
        }
    }

    async fn wait_for(queue: &QueueManager, id: &str, pred: impl Fn(&Job) -> bool) -> Job {
        for _ in 0..400 {
            if let Some(job) = queue.list().into_iter().find(|j| j.id == id) {
                if pred(&job) {
                    return job;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "timed out waiting for job {id}: {:?}",
            queue.list().into_iter().find(|j| j.id == id)
        );
    }

    #[tokio::test]
    async fn enqueue_validates_input() {
        let h = harness(false, |_| {}).await;
        assert_eq!(
            h.queue.enqueue(vec![]).unwrap_err().code,
            ErrorCode::InvalidInput
        );
        let mut bad = request("aqz-KE-bpKQ");
        bad.url = "https://example.com/x".into();
        assert_eq!(
            h.queue.enqueue(vec![bad]).unwrap_err().code,
            ErrorCode::UnsupportedUrl
        );
        let mut bad_dir = request("aqz-KE-bpKQ");
        bad_dir.output_dir = Some("/definitely/missing/dir".into());
        assert_eq!(
            h.queue.enqueue(vec![bad_dir]).unwrap_err().code,
            ErrorCode::InvalidPath
        );
        let mut bad_tpl = request("aqz-KE-bpKQ");
        bad_tpl.options.filename_template = Some("../../x.%(ext)s".into());
        assert_eq!(
            h.queue.enqueue(vec![bad_tpl]).unwrap_err().code,
            ErrorCode::InvalidInput
        );
        assert!(h.queue.list().is_empty());
    }

    #[tokio::test]
    async fn reorders_and_cancels_pending_jobs() {
        let h = harness(false, |_| {}).await;
        let jobs = h
            .queue
            .enqueue(vec![
                request("aaaaaaaaaaa"),
                request("bbbbbbbbbbb"),
                request("ccccccccccc"),
            ])
            .unwrap();
        h.queue.move_to(&jobs[2].id, 0).unwrap();
        let order: Vec<String> = h.queue.list().into_iter().map(|j| j.id).collect();
        assert_eq!(
            order,
            vec![jobs[2].id.clone(), jobs[0].id.clone(), jobs[1].id.clone()]
        );
        h.queue.move_to(&jobs[2].id, 99).unwrap();
        assert_eq!(h.queue.list().last().unwrap().id, jobs[2].id);

        h.queue.pause(&jobs[0].id).unwrap();
        assert_eq!(h.queue.list()[0].state, JobState::Pending);
        h.queue.resume(&jobs[0].id).unwrap();
        assert_eq!(h.queue.list()[0].state, JobState::Queued);
        h.queue.cancel(&jobs[1].id).unwrap();
        assert_eq!(h.queue.list()[1].state, JobState::Cancelled);
        assert_eq!(h.queue.clear_finished(), 1);
        assert_eq!(h.queue.list().len(), 2);
        h.queue.remove(&[jobs[0].id.clone()]).unwrap();
        assert_eq!(h.queue.list().len(), 1);
    }

    #[tokio::test]
    async fn pending_jobs_survive_restart() {
        let h = harness(false, |_| {}).await;
        let jobs = h
            .queue
            .enqueue(vec![request("aaaaaaaaaaa"), request("bbbbbbbbbbb")])
            .unwrap();
        h.queue.pause(&jobs[1].id).unwrap();
        let deps = Arc::new(DependencyManager::new(h._dir.path().join("bin2"), false).unwrap());
        let restored = QueueManager::new(
            h.storage.clone(),
            deps,
            h.settings.clone(),
            Arc::new(AnalysisCache::default()),
            QueuePaths {
                jobs_dir: h.jobs_dir.clone(),
                ytdlp_cache: h._dir.path().join("c"),
                thumbnails_dir: h._dir.path().join("t"),
            },
            h.sink.clone(),
        );
        restored.start();
        let list = restored.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].state, JobState::Pending);
        assert_eq!(list[0].title, "Title aaaaaaaaaaa");
        // Without yt-dlp the restored job fails cleanly with a dependency error.
        let failed = wait_for(&restored, &list[0].id, |j| j.state == JobState::Failed).await;
        assert_eq!(failed.error.unwrap().code, ErrorCode::DependencyMissing);
    }

    #[tokio::test]
    async fn missing_dependencies_fail_with_clear_error() {
        let h = harness(false, |_| {}).await;
        h.queue.start();
        let job = h
            .queue
            .enqueue(vec![request("aqz-KE-bpKQ")])
            .unwrap()
            .remove(0);
        let job = wait_for(&h.queue, &job.id, |j| j.state == JobState::Failed).await;
        assert_eq!(job.error.unwrap().code, ErrorCode::DependencyMissing);
        assert_eq!(
            h.sink.notifications.lock().unwrap().as_slice(),
            ["Download failed"]
        );
    }

    #[tokio::test]
    async fn downloads_end_to_end_with_real_progress() {
        let h = harness(true, |_| {}).await;
        h.queue.start();
        let job = h
            .queue
            .enqueue(vec![request("okokokokok1")])
            .unwrap()
            .remove(0);
        let done = wait_for(&h.queue, &job.id, |j| j.state.is_finished()).await;
        assert_eq!(done.state, JobState::Completed, "{:?}", done.error);
        let path = PathBuf::from(done.output_path.unwrap());
        assert!(path.starts_with(&h.out));
        assert!(path.is_file());
        assert!(done.file_size.unwrap() > 0);
        assert_eq!(
            done.title, "Fake Video okokokokok1",
            "metadata from analysis is applied"
        );
        let updates = h.sink.updates.lock().unwrap().clone();
        assert!(updates.iter().any(|u| u.state == JobState::Analyzing));
        assert!(updates.iter().any(|u| u.state == JobState::Downloading
            && u.progress
                .download_fraction
                .is_some_and(|f| f > 0.0 && f < 1.0)));
        assert!(updates.iter().any(|u| u.state == JobState::Merging));
        let history = h.storage.get_history(&job.id).unwrap().unwrap();
        assert_eq!(history.status, "completed");
        assert!(history.file_exists);
        assert!(
            !h.jobs_dir.join(&job.id).exists(),
            "work folder is cleaned up"
        );
    }

    #[tokio::test]
    async fn audio_downloads_use_requested_format() {
        let h = harness(true, |_| {}).await;
        h.queue.start();
        let mut req = request("okokokokok2");
        req.options.media_type = MediaType::Audio;
        req.options.audio_format = formats::AudioFormat::Mp3;
        let job = h.queue.enqueue(vec![req]).unwrap().remove(0);
        let done = wait_for(&h.queue, &job.id, |j| j.state.is_finished()).await;
        assert_eq!(done.state, JobState::Completed, "{:?}", done.error);
        assert!(done.output_path.unwrap().ends_with(".mp3"));
        let updates = h.sink.updates.lock().unwrap().clone();
        assert!(updates.iter().any(|u| u.state == JobState::Converting));
    }

    #[tokio::test]
    async fn duplicate_downloads_are_renamed_or_skipped() {
        let h = harness(true, |_| {}).await;
        h.queue.start();
        let first = h
            .queue
            .enqueue(vec![request("okokokokok3")])
            .unwrap()
            .remove(0);
        let a = wait_for(&h.queue, &first.id, |j| j.state.is_finished()).await;
        let second = h
            .queue
            .enqueue(vec![request("okokokokok3")])
            .unwrap()
            .remove(0);
        let b = wait_for(&h.queue, &second.id, |j| j.state.is_finished()).await;
        assert_ne!(a.output_path, b.output_path);
        assert!(b.output_path.as_deref().unwrap().contains(" (1)."));

        h.settings.write().unwrap().duplicate_policy = DuplicatePolicy::Skip;
        let third = h
            .queue
            .enqueue(vec![request("okokokokok3")])
            .unwrap()
            .remove(0);
        let c = wait_for(&h.queue, &third.id, |j| j.state.is_finished()).await;
        assert_eq!(c.state, JobState::Completed);
        assert_eq!(c.output_path, a.output_path);
        assert!(c.notes.iter().any(|n| n.contains("already exists")));
    }

    #[tokio::test]
    async fn concurrent_downloads_respect_limit() {
        let h = harness(true, |s| s.max_concurrent_downloads = 2).await;
        h.queue.start();
        let jobs = h
            .queue
            .enqueue(
                (1..=4)
                    .map(|i| request(&format!("slowslowsl{i}")))
                    .collect(),
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(h.queue.running_count(), 2);
        for job in &jobs {
            h.queue.cancel(&job.id).unwrap();
        }
        for job in &jobs {
            let j = wait_for(&h.queue, &job.id, |j| {
                j.state.is_finished() && !j.state.is_running()
            })
            .await;
            assert_eq!(j.state, JobState::Cancelled);
        }
    }

    #[tokio::test]
    async fn cancellation_stops_the_process_and_removes_partial_files() {
        let h = harness(true, |_| {}).await;
        h.queue.start();
        let job = h
            .queue
            .enqueue(vec![request("slowslowsl0")])
            .unwrap()
            .remove(0);
        wait_for(&h.queue, &job.id, |j| {
            j.state == JobState::Downloading && j.progress.downloaded_bytes > 0
        })
        .await;
        assert!(h.jobs_dir.join(&job.id).exists());
        h.queue.cancel(&job.id).unwrap();
        let j = wait_for(&h.queue, &job.id, |j| j.state == JobState::Cancelled).await;
        assert!(j.output_path.is_none());
        assert!(!h.jobs_dir.join(&job.id).exists());
        assert_eq!(
            h.storage.get_history(&job.id).unwrap().unwrap().status,
            "cancelled"
        );
    }

    #[tokio::test]
    async fn pause_keeps_partial_data_and_resume_finishes() {
        let h = harness(true, |_| {}).await;
        h.queue.start();
        let job = h
            .queue
            .enqueue(vec![request("slowslowsl9")])
            .unwrap()
            .remove(0);
        wait_for(&h.queue, &job.id, |j| j.progress.downloaded_bytes > 0).await;
        h.queue.pause(&job.id).unwrap();
        let paused = wait_for(&h.queue, &job.id, |j| j.state == JobState::Pending).await;
        assert!(paused.paused);
        assert!(
            h.jobs_dir.join(&job.id).exists(),
            "partial data is kept for resuming"
        );
    }

    #[tokio::test]
    async fn transient_errors_are_retried_automatically() {
        let h = harness(true, |s| s.retry_attempts = 2).await;
        h.queue.start();
        // The fake fails with a network error on its first attempt for this id.
        let job = h
            .queue
            .enqueue(vec![request("netfailnet1")])
            .unwrap()
            .remove(0);
        let done = wait_for(&h.queue, &job.id, |j| j.state.is_finished()).await;
        assert_eq!(done.state, JobState::Completed, "{:?}", done.error);
        assert_eq!(done.attempts, 2);
    }

    #[tokio::test]
    async fn permanent_errors_fail_without_retry() {
        let h = harness(true, |s| s.retry_attempts = 3).await;
        h.queue.start();
        let job = h
            .queue
            .enqueue(vec![request("privatevid1")])
            .unwrap()
            .remove(0);
        let done = wait_for(&h.queue, &job.id, |j| j.state.is_finished()).await;
        assert_eq!(done.state, JobState::Failed);
        assert_eq!(done.error.as_ref().unwrap().code, ErrorCode::PrivateContent);
        // The error surfaces during analysis, so no download attempt (and no retry) is made.
        assert_eq!(done.attempts, 0);
        assert!(!h
            .sink
            .updates
            .lock()
            .unwrap()
            .iter()
            .any(|u| u.progress.retry_label.is_some()));
        h.queue.retry(&job.id).unwrap();
        assert_eq!(h.queue.list()[0].attempts, 0);
    }

    #[test]
    fn analysis_cache_evicts_and_expires() {
        let cache = AnalysisCache::default();
        let media = MediaInfo {
            id: "x".into(),
            url: "u".into(),
            title: "t".into(),
            uploader: None,
            duration: None,
            thumbnail: None,
            thumbnail_local: None,
            platform: Platform::Youtube,
            was_live: false,
            upload_date: None,
            view_count: None,
            chapters: 0,
            formats: vec![],
            summary: Default::default(),
            subtitles: vec![],
            playlist_id: None,
            playlist_item: None,
        };
        for i in 0..250 {
            cache.put(&format!("u{i}"), None, media.clone());
        }
        assert!(cache.get("u249", None).is_some());
        assert!(cache.get("u0", None).is_none());
        assert!(cache.get("u249", Some(1)).is_none());
    }

    #[test]
    fn temporary_files_are_recognized() {
        assert!(is_temporary("video.f137.mp4.part"));
        assert!(is_temporary("video.mp4.ytdl"));
        assert!(is_temporary(PROGRESS_FILE));
        assert!(is_temporary("video.temp.mp4"));
        assert!(!is_temporary("video.en.vtt"));
    }
}
