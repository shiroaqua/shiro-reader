use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    future::Future,
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use shiro_application::library::book::dto::GetBookOutput;
use shiro_error_catalog::{
    AppLocale, CommandErrorPayload, ErrorCode, ErrorEventPayload, PairedClientError,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeekExt},
    sync::{mpsc, oneshot, watch},
    task::{Id as TaskId, JoinSet},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    cloud_client::is_session_invalid_code,
    config::clamp_upload_concurrency,
    dto::{
        UploadActiveStage, UploadPanelSnapshotDto, UploadSessionStatsDto, UploadTaskDto,
        UploadTaskStatusDto,
    },
    error_sink::ErrorDispatcher,
    local_import::{
        FileProbeStatus, ImportProgressCallback, ImportTarget, StagedBook, derive_book_title,
        resolve_staged_book_title, stage_book,
    },
};

const AUTO_DISMISS_DELAY: Duration = Duration::from_millis(1_000);
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100);
const SPEED_MIN_ELAPSED: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadSourceLocator {
    Path(PathBuf),
    Uri(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueUploadItem {
    pub file_name: String,
    pub total_bytes: Option<u64>,
    pub locator: UploadSourceLocator,
}

pub type BoxedUploadReader = Pin<Box<dyn AsyncRead + Unpin + Send>>;
pub type OpenUploadSourceFuture =
    Pin<Box<dyn Future<Output = Result<(BoxedUploadReader, Option<u64>), ErrorCode>> + Send>>;

pub trait UploadSourceOpener: Send + Sync + 'static {
    fn open_source(&self, locator: &UploadSourceLocator) -> OpenUploadSourceFuture;
}

#[derive(Debug, Default)]
pub struct StdFsSourceOpener;

impl UploadSourceOpener for StdFsSourceOpener {
    fn open_source(&self, locator: &UploadSourceLocator) -> OpenUploadSourceFuture {
        let locator = locator.clone();
        Box::pin(async move {
            match locator {
                UploadSourceLocator::Path(path) => {
                    let file = File::open(&path)
                        .await
                        .map_err(|_| ErrorCode::ImportReadFailed)?;
                    let size = file.metadata().await.ok().map(|meta| meta.len());
                    Ok((Box::pin(file) as BoxedUploadReader, size))
                }
                UploadSourceLocator::Uri(_) => Err(ErrorCode::ImportReadFailed),
            }
        })
    }
}

pub(crate) type TargetResolver = Arc<dyn Fn() -> Result<ImportTarget, ErrorCode> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedUploadPaths {
    pub items: Vec<EnqueueUploadItem>,
    pub had_directory: bool,
}

fn is_supported_book_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf") || ext.eq_ignore_ascii_case("epub"))
}

fn file_name_from_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

fn normalized_relative_key(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Expands user-selected or dropped file/directory paths into concrete upload items.
///
/// - Individual files are always enqueued without extension pre-filtering so unsupported files
///   surface as per-task `BookFileUnsupported` failures inside the upload panel.
/// - Directories are traversed recursively: hidden children whose `DirEntry::file_name()` starts
///   with `.` are skipped (root ancestors are never inspected so paths under `~/.local` or
///   `/tmp/.tmp*` remain valid), symlink directory cycles and overlapping roots/files are
///   deduplicated via canonicalized path sets, and matching `.pdf`/`.epub` files are sorted stably
///   by full relative path.
pub fn expand_upload_paths(paths: Vec<PathBuf>) -> ExpandedUploadPaths {
    let mut items = Vec::new();
    let mut had_directory = false;
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut seen_files: HashSet<PathBuf> = HashSet::new();

    for root in paths {
        let Ok(root_meta) = fs::metadata(&root) else {
            if seen_files.insert(root.clone()) {
                items.push(EnqueueUploadItem {
                    file_name: file_name_from_path(&root),
                    total_bytes: None,
                    locator: UploadSourceLocator::Path(root),
                });
            }
            continue;
        };

        if root_meta.is_file() {
            if seen_files.insert(canonical_or_raw(&root)) {
                items.push(EnqueueUploadItem {
                    file_name: file_name_from_path(&root),
                    total_bytes: Some(root_meta.len()),
                    locator: UploadSourceLocator::Path(root),
                });
            }
            continue;
        }

        if !root_meta.is_dir() {
            continue;
        }

        had_directory = true;
        if !visited_dirs.insert(canonical_or_raw(&root)) {
            continue;
        }

        let mut stack = vec![root.clone()];
        let mut discovered: Vec<(String, PathBuf, Option<u64>)> = Vec::new();

        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let child_name = entry.file_name();
                if child_name.to_string_lossy().starts_with('.') {
                    continue;
                }

                let entry_path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };

                if file_type.is_symlink() {
                    let Ok(target_meta) = fs::metadata(&entry_path) else {
                        continue;
                    };
                    if target_meta.is_dir() {
                        if visited_dirs.insert(canonical_or_raw(&entry_path)) {
                            stack.push(entry_path);
                        }
                    } else if target_meta.is_file()
                        && is_supported_book_extension(&entry_path)
                        && seen_files.insert(canonical_or_raw(&entry_path))
                    {
                        discovered.push((
                            normalized_relative_key(&root, &entry_path),
                            entry_path,
                            Some(target_meta.len()),
                        ));
                    }
                } else if file_type.is_dir() {
                    if visited_dirs.insert(canonical_or_raw(&entry_path)) {
                        stack.push(entry_path);
                    }
                } else if file_type.is_file()
                    && is_supported_book_extension(&entry_path)
                    && seen_files.insert(canonical_or_raw(&entry_path))
                {
                    let len = entry.metadata().ok().map(|meta| meta.len());
                    discovered.push((normalized_relative_key(&root, &entry_path), entry_path, len));
                }
            }
        }

        discovered.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, path, total_bytes) in discovered {
            items.push(EnqueueUploadItem {
                file_name: file_name_from_path(&path),
                total_bytes,
                locator: UploadSourceLocator::Path(path),
            });
        }
    }

    ExpandedUploadPaths {
        items,
        had_directory,
    }
}

#[derive(Clone)]
enum FlightState {
    Uploading { bytes_sent: u64, total_bytes: u64 },
    Finished(Result<(), ErrorCode>),
}

struct FlightEntry {
    flight_id: u64,
    rx: watch::Receiver<FlightState>,
}

struct ClientHashFlightMap {
    next_flight_id: AtomicU64,
    entries: Mutex<HashMap<String, FlightEntry>>,
}

impl ClientHashFlightMap {
    fn new() -> Self {
        Self {
            next_flight_id: AtomicU64::new(1),
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn acquire_or_join(self: &Arc<Self>, hash: &str, total_bytes: u64) -> FlightRole {
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = guard.get(hash) {
            return FlightRole::Follower(existing.rx.clone());
        }

        let flight_id = self.next_flight_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = watch::channel(FlightState::Uploading {
            bytes_sent: 0,
            total_bytes,
        });
        guard.insert(hash.to_owned(), FlightEntry { flight_id, rx });
        drop(guard);

        FlightRole::Leader(FlightLeaderGuard {
            map: Arc::clone(self),
            hash: hash.to_owned(),
            flight_id,
            tx: Some(tx),
            finished: false,
        })
    }
}

enum FlightRole {
    Leader(FlightLeaderGuard),
    Follower(watch::Receiver<FlightState>),
}

struct FlightLeaderGuard {
    map: Arc<ClientHashFlightMap>,
    hash: String,
    flight_id: u64,
    tx: Option<watch::Sender<FlightState>>,
    finished: bool,
}

impl FlightLeaderGuard {
    fn update_progress(&self, bytes_sent: u64, total_bytes: u64) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(FlightState::Uploading {
                bytes_sent,
                total_bytes,
            });
        }
    }

    /// Removes the flight entry BEFORE broadcasting completion so woken followers can immediately
    /// re-compete for leadership without ever observing a stale map entry.
    fn finish(mut self, result: Result<(), ErrorCode>) {
        self.remove_entry();
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(FlightState::Finished(result));
        }
        self.finished = true;
    }

    fn remove_entry(&self) {
        let mut guard = self.map.entries.lock().unwrap_or_else(|e| e.into_inner());
        if guard
            .get(&self.hash)
            .is_some_and(|entry| entry.flight_id == self.flight_id)
        {
            guard.remove(&self.hash);
        }
    }
}

impl Drop for FlightLeaderGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.remove_entry();
            if let Some(tx) = self.tx.take() {
                let _ = tx.send(FlightState::Finished(Err(ErrorCode::ImportCancelled)));
            }
            self.finished = true;
        }
    }
}

enum UploadCommand {
    Enqueue {
        items: Vec<EnqueueUploadItem>,
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    RetryTask {
        task_id: u64,
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    CancelTask {
        task_id: u64,
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    CancelAllRemaining {
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    DismissTask {
        task_id: u64,
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    DismissFailed {
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    ClearCompleted {
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    SetConcurrency {
        concurrency: usize,
        reply: oneshot::Sender<UploadPanelSnapshotDto>,
    },
    RelocalizeErrors {
        locale: AppLocale,
    },
    AbortAll {
        ack: Option<oneshot::Sender<()>>,
    },
}

enum WorkerUpdate {
    Stage {
        task_id: u64,
        is_cloud: bool,
        stage: UploadActiveStage,
        total_bytes: Option<u64>,
    },
    Progress {
        task_id: u64,
        stage: UploadActiveStage,
        bytes_processed: u64,
        total_bytes: Option<u64>,
    },
}

struct WorkerOutcome {
    task_id: u64,
    result: Result<GetBookOutput, ErrorCode>,
}

struct ActiveMetrics {
    is_cloud: bool,
    stage: UploadActiveStage,
    stage_started_at: Instant,
    bytes_processed: u64,
    total_bytes: Option<u64>,
    progress_percent: u8,
    speed_bytes_per_sec: Option<u64>,
    eta_seconds: Option<u64>,
}

enum TaskState {
    Queued,
    Active(ActiveMetrics),
    Succeeded { book_id: String, title: String },
    Failed { code: ErrorCode, error: ErrorEventPayload },
}

struct TaskRecord {
    id: u64,
    item: EnqueueUploadItem,
    state: TaskState,
}

impl TaskRecord {
    fn to_dto(&self) -> UploadTaskDto {
        let status = match &self.state {
            TaskState::Queued => UploadTaskStatusDto::Queued,
            TaskState::Active(metrics) => UploadTaskStatusDto::Active {
                stage: metrics.stage,
                progress_percent: metrics.progress_percent,
                bytes_processed: metrics.bytes_processed,
                total_bytes: metrics.total_bytes,
                speed_bytes_per_sec: metrics.speed_bytes_per_sec,
                eta_seconds: metrics.eta_seconds,
            },
            TaskState::Succeeded { book_id, title } => UploadTaskStatusDto::Succeeded {
                book_id: book_id.clone(),
                title: title.clone(),
            },
            TaskState::Failed { error, .. } => UploadTaskStatusDto::Failed {
                error: error.clone(),
            },
        };
        UploadTaskDto {
            id: self.id,
            file_name: self.item.file_name.clone(),
            status,
        }
    }
}

fn initial_snapshot(concurrency: usize) -> UploadPanelSnapshotDto {
    UploadPanelSnapshotDto {
        revision: 0,
        visible: false,
        concurrency,
        completed_counter: 0,
        stats: UploadSessionStatsDto::default(),
        has_dismissible_errors: false,
        tasks: Vec::new(),
    }
}

fn stage_range(is_cloud: bool, stage: UploadActiveStage) -> (u8, u8) {
    match (is_cloud, stage) {
        (false, UploadActiveStage::Staging) => (0, 80),
        (false, UploadActiveStage::Uploading | UploadActiveStage::Finalizing) => (80, 95),
        (true, UploadActiveStage::Staging) => (0, 15),
        (true, UploadActiveStage::Uploading) => (15, 95),
        (true, UploadActiveStage::Finalizing) => (95, 99),
    }
}

fn compute_progress_percent(
    is_cloud: bool,
    stage: UploadActiveStage,
    bytes_processed: u64,
    total_bytes: Option<u64>,
) -> u8 {
    let (start, end) = stage_range(is_cloud, stage);
    let Some(total) = total_bytes.filter(|&total| total > 0) else {
        return start;
    };
    let span = u128::from(end.saturating_sub(start));
    let clamped = bytes_processed.min(total);
    let delta = ((u128::from(clamped) * span) / u128::from(total)) as u8;
    start.saturating_add(delta).min(end)
}

struct CoordinatorState {
    tasks: BTreeMap<u64, TaskRecord>,
    pending_queue: VecDeque<u64>,
    active_workers: HashMap<u64, CancellationToken>,
    cancelling_workers: HashSet<u64>,
    join_id_to_task: HashMap<TaskId, u64>,
    auto_dismiss_deadlines: HashMap<u64, Instant>,
    join_set: JoinSet<WorkerOutcome>,
    max_concurrency: usize,
    next_task_id: u64,
    revision: u64,
    visible: bool,
    completed_counter: u64,
    session_total: usize,
    session_succeeded: usize,
    session_failed: usize,
    locale: AppLocale,
    last_progress_emit: Option<Instant>,
    progress_dirty: bool,
    drain_waiters: Vec<oneshot::Sender<()>>,
    flight_map: Arc<ClientHashFlightMap>,
    source_opener: Arc<dyn UploadSourceOpener>,
    resolve_target: TargetResolver,
    error_dispatcher: Arc<ErrorDispatcher>,
    latest_snapshot: Arc<RwLock<UploadPanelSnapshotDto>>,
    worker_tx: mpsc::UnboundedSender<WorkerUpdate>,
    auto_dismiss_delay: Duration,
    retain_completed_uploads: bool,
}

impl CoordinatorState {
    fn build_snapshot(&self) -> UploadPanelSnapshotDto {
        let remaining = self.pending_queue.len() + self.active_workers.len();
        let active = self.active_workers.len();
        let has_dismissible_errors = self
            .tasks
            .values()
            .any(|task| matches!(task.state, TaskState::Failed { .. }));
        UploadPanelSnapshotDto {
            revision: self.revision,
            visible: self.visible,
            concurrency: self.max_concurrency,
            completed_counter: self.completed_counter,
            stats: UploadSessionStatsDto {
                total: self.session_total,
                remaining,
                active,
                succeeded: self.session_succeeded,
                failed: self.session_failed,
            },
            has_dismissible_errors,
            tasks: self.tasks.values().map(TaskRecord::to_dto).collect(),
        }
    }

    fn publish_snapshot(&mut self, force: bool) -> UploadPanelSnapshotDto {
        let now = Instant::now();
        if !force
            && let Some(last) = self.last_progress_emit
            && now.saturating_duration_since(last) < PROGRESS_EMIT_INTERVAL
        {
            self.progress_dirty = true;
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        }

        self.revision = self.revision.saturating_add(1);
        self.last_progress_emit = Some(now);
        self.progress_dirty = false;
        let snapshot = self.build_snapshot();
        *self
            .latest_snapshot
            .write()
            .unwrap_or_else(|e| e.into_inner()) = snapshot.clone();
        self.error_dispatcher.emit_upload_state(&snapshot);
        snapshot
    }

    fn maybe_hide_and_reset(&mut self) {
        if self.tasks.is_empty()
            && self.active_workers.is_empty()
            && self.pending_queue.is_empty()
        {
            self.visible = false;
            self.session_total = 0;
            self.session_succeeded = 0;
            self.session_failed = 0;
            self.auto_dismiss_deadlines.clear();
        }
    }

    fn notify_drain_waiters_if_empty(&mut self) {
        if self.join_set.is_empty() {
            for waiter in self.drain_waiters.drain(..) {
                let _ = waiter.send(());
            }
        }
    }

    fn pump_queue(&mut self) {
        while self.active_workers.len() < self.max_concurrency {
            let Some(task_id) = self.pending_queue.pop_front() else {
                break;
            };
            let Some(record) = self.tasks.get_mut(&task_id) else {
                continue;
            };
            if !matches!(record.state, TaskState::Queued) {
                continue;
            }

            let target = match (self.resolve_target)() {
                Ok(target) => target,
                Err(code) => {
                    let error = PairedClientError::new(
                        code,
                        self.locale,
                        Uuid::new_v4().to_string(),
                        Some(record.item.file_name.clone()),
                    )
                    .event;
                    record.state = TaskState::Failed { code, error };
                    self.session_failed += 1;
                    continue;
                }
            };

            let is_cloud = target.is_cloud();
            let total_bytes = record.item.total_bytes;
            record.state = TaskState::Active(ActiveMetrics {
                is_cloud,
                stage: UploadActiveStage::Staging,
                stage_started_at: Instant::now(),
                bytes_processed: 0,
                total_bytes,
                progress_percent: 0,
                speed_bytes_per_sec: None,
                eta_seconds: None,
            });

            let cancel = CancellationToken::new();
            self.active_workers.insert(task_id, cancel.clone());

            let item = record.item.clone();
            let opener = Arc::clone(&self.source_opener);
            let flight_map = Arc::clone(&self.flight_map);
            let worker_tx = self.worker_tx.clone();

            let handle = self.join_set.spawn(async move {
                let result =
                    run_upload_worker(task_id, item, target, opener, flight_map, cancel, worker_tx)
                        .await;
                WorkerOutcome { task_id, result }
            });
            self.join_id_to_task.insert(handle.id(), task_id);
        }
    }

    fn handle_enqueue(&mut self, items: Vec<EnqueueUploadItem>) -> UploadPanelSnapshotDto {
        if items.is_empty() {
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        }

        if self.retain_completed_uploads
            && self.pending_queue.is_empty()
            && self.active_workers.is_empty()
        {
            self.tasks
                .retain(|_, record| !matches!(record.state, TaskState::Succeeded { .. }));
            self.session_succeeded = 0;
            self.session_failed = self
                .tasks
                .values()
                .filter(|record| matches!(record.state, TaskState::Failed { .. }))
                .count();
            self.session_total = self.tasks.len();
        }

        self.visible = true;
        for item in items {
            let id = self.next_task_id;
            self.next_task_id = self.next_task_id.saturating_add(1);
            self.tasks.insert(
                id,
                TaskRecord {
                    id,
                    item,
                    state: TaskState::Queued,
                },
            );
            self.pending_queue.push_back(id);
            self.session_total += 1;
        }
        self.pump_queue();
        self.publish_snapshot(true)
    }

    fn handle_retry_task(&mut self, task_id: u64) -> UploadPanelSnapshotDto {
        let retried = if let Some(record) = self.tasks.get_mut(&task_id)
            && matches!(record.state, TaskState::Failed { .. })
        {
            record.state = TaskState::Queued;
            self.session_failed = self.session_failed.saturating_sub(1);
            self.pending_queue.push_back(task_id);
            self.visible = true;
            true
        } else {
            false
        };

        if retried {
            self.pump_queue();
            self.publish_snapshot(true)
        } else {
            self.latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    fn cancel_task_internal(&mut self, task_id: u64) -> bool {
        let Some(record) = self.tasks.get(&task_id) else {
            return false;
        };
        match record.state {
            TaskState::Queued => {
                self.pending_queue.retain(|&id| id != task_id);
                self.tasks.remove(&task_id);
                self.session_total = self.session_total.saturating_sub(1);
                true
            }
            TaskState::Active(_) => {
                if let Some(token) = self.active_workers.remove(&task_id) {
                    token.cancel();
                    self.cancelling_workers.insert(task_id);
                }
                self.tasks.remove(&task_id);
                self.session_total = self.session_total.saturating_sub(1);
                true
            }
            TaskState::Succeeded { .. } | TaskState::Failed { .. } => false,
        }
    }

    fn handle_cancel_task(&mut self, task_id: u64) -> UploadPanelSnapshotDto {
        if self.cancel_task_internal(task_id) {
            self.pump_queue();
            self.maybe_hide_and_reset();
            self.publish_snapshot(true)
        } else {
            self.latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    fn handle_cancel_all_remaining(&mut self) -> UploadPanelSnapshotDto {
        let mut changed = false;

        let queued: Vec<u64> = self.pending_queue.drain(..).collect();
        for task_id in queued {
            if self.tasks.remove(&task_id).is_some() {
                self.session_total = self.session_total.saturating_sub(1);
                changed = true;
            }
        }

        let active_entries: Vec<(u64, CancellationToken)> = self.active_workers.drain().collect();
        for (task_id, token) in active_entries {
            token.cancel();
            self.cancelling_workers.insert(task_id);
            if self.tasks.remove(&task_id).is_some() {
                self.session_total = self.session_total.saturating_sub(1);
                changed = true;
            }
        }

        if changed {
            self.maybe_hide_and_reset();
            self.publish_snapshot(true)
        } else {
            self.latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    fn handle_dismiss_task(&mut self, task_id: u64) -> UploadPanelSnapshotDto {
        let Some(record) = self.tasks.get(&task_id) else {
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        };

        let dismissed = match record.state {
            TaskState::Failed { .. } => {
                self.tasks.remove(&task_id);
                self.session_failed = self.session_failed.saturating_sub(1);
                self.session_total = self.session_total.saturating_sub(1);
                true
            }
            TaskState::Succeeded { .. } => {
                self.tasks.remove(&task_id);
                self.auto_dismiss_deadlines.remove(&task_id);
                true
            }
            TaskState::Queued | TaskState::Active(_) => false,
        };

        if dismissed {
            self.maybe_hide_and_reset();
            self.publish_snapshot(true)
        } else {
            self.latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }
    }

    fn handle_dismiss_failed(&mut self) -> UploadPanelSnapshotDto {
        let failed_ids: Vec<u64> = self
            .tasks
            .iter()
            .filter_map(|(&id, rec)| matches!(rec.state, TaskState::Failed { .. }).then_some(id))
            .collect();

        if failed_ids.is_empty() {
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        }

        for id in failed_ids {
            self.tasks.remove(&id);
            self.session_failed = self.session_failed.saturating_sub(1);
            self.session_total = self.session_total.saturating_sub(1);
        }

        self.maybe_hide_and_reset();
        self.publish_snapshot(true)
    }

    fn handle_clear_completed(&mut self) -> UploadPanelSnapshotDto {
        if !self.pending_queue.is_empty()
            || !self.active_workers.is_empty()
            || (self.tasks.is_empty() && !self.visible)
        {
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        }

        self.tasks.clear();
        self.auto_dismiss_deadlines.clear();
        self.visible = false;
        self.session_total = 0;
        self.session_succeeded = 0;
        self.session_failed = 0;
        self.publish_snapshot(true)
    }

    fn handle_set_concurrency(&mut self, concurrency: usize) -> UploadPanelSnapshotDto {
        let clamped = clamp_upload_concurrency(concurrency);
        if clamped == self.max_concurrency {
            return self
                .latest_snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
        }
        self.max_concurrency = clamped;
        self.pump_queue();
        self.publish_snapshot(true)
    }

    fn handle_relocalize_errors(&mut self, locale: AppLocale) {
        self.locale = locale;
        let mut changed = false;
        for record in self.tasks.values_mut() {
            if let TaskState::Failed { code, error } = &mut record.state {
                error.locale = locale;
                error.message = code.localize(locale).to_owned();
                changed = true;
            }
        }
        if changed {
            self.publish_snapshot(true);
        }
    }

    fn handle_abort_all(&mut self, ack: Option<oneshot::Sender<()>>) {
        let had_state = self.visible
            || !self.tasks.is_empty()
            || !self.pending_queue.is_empty()
            || !self.active_workers.is_empty()
            || self.session_total > 0
            || self.session_succeeded > 0
            || self.session_failed > 0;

        self.pending_queue.clear();

        let active_entries: Vec<(u64, CancellationToken)> = self.active_workers.drain().collect();
        for (task_id, token) in active_entries {
            token.cancel();
            self.cancelling_workers.insert(task_id);
        }

        self.tasks.clear();
        self.auto_dismiss_deadlines.clear();
        self.visible = false;
        self.session_total = 0;
        self.session_succeeded = 0;
        self.session_failed = 0;
        self.progress_dirty = false;

        if had_state {
            self.publish_snapshot(true);
        }

        if let Some(tx) = ack {
            if self.join_set.is_empty() {
                let _ = tx.send(());
            } else {
                self.drain_waiters.push(tx);
            }
        }
    }

    fn handle_worker_update(&mut self, update: WorkerUpdate) {
        match update {
            WorkerUpdate::Stage {
                task_id,
                is_cloud,
                stage,
                total_bytes,
            } => {
                let Some(record) = self.tasks.get_mut(&task_id) else {
                    return;
                };
                if let TaskState::Active(metrics) = &mut record.state {
                    metrics.is_cloud = is_cloud;
                    metrics.stage = stage;
                    metrics.stage_started_at = Instant::now();
                    metrics.bytes_processed = 0;
                    if total_bytes.is_some() {
                        metrics.total_bytes = total_bytes;
                    }
                    metrics.progress_percent =
                        compute_progress_percent(is_cloud, stage, 0, metrics.total_bytes);
                    metrics.speed_bytes_per_sec = None;
                    metrics.eta_seconds = None;
                    self.publish_snapshot(true);
                }
            }
            WorkerUpdate::Progress {
                task_id,
                stage,
                bytes_processed,
                total_bytes,
            } => {
                let Some(record) = self.tasks.get_mut(&task_id) else {
                    return;
                };
                if let TaskState::Active(metrics) = &mut record.state {
                    if metrics.stage != stage {
                        return;
                    }
                    metrics.bytes_processed = metrics.bytes_processed.max(bytes_processed);
                    if total_bytes.is_some() {
                        metrics.total_bytes = total_bytes;
                    }
                    metrics.progress_percent = compute_progress_percent(
                        metrics.is_cloud,
                        metrics.stage,
                        metrics.bytes_processed,
                        metrics.total_bytes,
                    );

                    let elapsed = Instant::now().saturating_duration_since(metrics.stage_started_at);
                    if elapsed >= SPEED_MIN_ELAPSED && metrics.bytes_processed > 0 {
                        let elapsed_ms = elapsed.as_millis().max(1);
                        let speed =
                            ((u128::from(metrics.bytes_processed) * 1_000) / elapsed_ms) as u64;
                        if speed > 0 {
                            metrics.speed_bytes_per_sec = Some(speed);
                            metrics.eta_seconds = metrics.total_bytes.map(|total| {
                                total
                                    .saturating_sub(metrics.bytes_processed)
                                    .div_ceil(speed)
                            });
                        } else {
                            metrics.speed_bytes_per_sec = None;
                            metrics.eta_seconds = None;
                        }
                    } else {
                        metrics.speed_bytes_per_sec = None;
                        metrics.eta_seconds = None;
                    }

                    self.publish_snapshot(false);
                }
            }
        }
    }

    fn handle_worker_finished(&mut self, outcome: Result<WorkerOutcome, tokio::task::JoinError>) {
        match outcome {
            Ok(WorkerOutcome { task_id, result }) => {
                self.join_id_to_task.retain(|_, id| *id != task_id);
                let was_cancelling = self.cancelling_workers.remove(&task_id);
                self.active_workers.remove(&task_id);

                if !was_cancelling && self.tasks.contains_key(&task_id) {
                    match result {
                        Ok(book) => {
                            if let Some(record) = self.tasks.get_mut(&task_id) {
                                record.state = TaskState::Succeeded {
                                    book_id: book.id,
                                    title: book.title,
                                };
                                self.session_succeeded += 1;
                                self.completed_counter = self.completed_counter.saturating_add(1);
                                if !self.retain_completed_uploads {
                                    self.auto_dismiss_deadlines
                                        .insert(task_id, Instant::now() + self.auto_dismiss_delay);
                                }
                            }
                        }
                        Err(ErrorCode::ImportCancelled) => {
                            self.tasks.remove(&task_id);
                            self.session_total = self.session_total.saturating_sub(1);
                        }
                        Err(code) if is_session_invalid_code(code) => {
                            self.tasks.remove(&task_id);
                            self.session_total = self.session_total.saturating_sub(1);
                            self.handle_abort_all(None);
                        }
                        Err(code) => {
                            if let Some(record) = self.tasks.get_mut(&task_id) {
                                let error = PairedClientError::new(
                                    code,
                                    self.locale,
                                    Uuid::new_v4().to_string(),
                                    Some(record.item.file_name.clone()),
                                )
                                .event;
                                record.state = TaskState::Failed { code, error };
                                self.session_failed += 1;
                            }
                        }
                    }
                }
            }
            Err(join_err) => {
                if let Some(task_id) = self.join_id_to_task.remove(&join_err.id()) {
                    let was_cancelling = self.cancelling_workers.remove(&task_id);
                    self.active_workers.remove(&task_id);
                    if !was_cancelling
                        && let Some(record) = self.tasks.get_mut(&task_id)
                    {
                        let code = ErrorCode::CommonInternalServerError;
                        let error = PairedClientError::new(
                            code,
                            self.locale,
                            Uuid::new_v4().to_string(),
                            Some(record.item.file_name.clone()),
                        )
                        .event;
                        record.state = TaskState::Failed { code, error };
                        self.session_failed += 1;
                    }
                }
            }
        }

        self.pump_queue();
        self.maybe_hide_and_reset();
        self.notify_drain_waiters_if_empty();
        self.publish_snapshot(true);
    }

    fn flush_expired_auto_dismiss(&mut self) {
        let now = Instant::now();
        let expired: Vec<u64> = self
            .auto_dismiss_deadlines
            .iter()
            .filter_map(|(&id, &deadline)| (deadline <= now).then_some(id))
            .collect();
        if expired.is_empty() {
            return;
        }
        for id in expired {
            self.auto_dismiss_deadlines.remove(&id);
            if let Some(record) = self.tasks.get(&id)
                && matches!(record.state, TaskState::Succeeded { .. })
            {
                self.tasks.remove(&id);
            }
        }
        self.maybe_hide_and_reset();
        self.publish_snapshot(true);
    }

    fn next_auto_dismiss_deadline(&self) -> Option<Instant> {
        self.auto_dismiss_deadlines.values().copied().min()
    }

    fn next_progress_flush_deadline(&self) -> Option<Instant> {
        if !self.progress_dirty {
            return None;
        }
        Some(
            self.last_progress_emit
                .map(|last| last + PROGRESS_EMIT_INTERVAL)
                .unwrap_or_else(Instant::now),
        )
    }

    async fn run(
        mut self,
        mut cmd_rx: mpsc::UnboundedReceiver<UploadCommand>,
        mut worker_rx: mpsc::UnboundedReceiver<WorkerUpdate>,
    ) {
        loop {
            let next_dismiss = self.next_auto_dismiss_deadline();
            let next_progress = self.next_progress_flush_deadline();
            let has_workers = !self.join_set.is_empty();

            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(UploadCommand::Enqueue { items, reply }) => {
                            let snap = self.handle_enqueue(items);
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::RetryTask { task_id, reply }) => {
                            let snap = self.handle_retry_task(task_id);
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::CancelTask { task_id, reply }) => {
                            let snap = self.handle_cancel_task(task_id);
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::CancelAllRemaining { reply }) => {
                            let snap = self.handle_cancel_all_remaining();
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::DismissTask { task_id, reply }) => {
                            let snap = self.handle_dismiss_task(task_id);
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::DismissFailed { reply }) => {
                            let snap = self.handle_dismiss_failed();
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::ClearCompleted { reply }) => {
                            let snap = self.handle_clear_completed();
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::SetConcurrency { concurrency, reply }) => {
                            let snap = self.handle_set_concurrency(concurrency);
                            let _ = reply.send(snap);
                        }
                        Some(UploadCommand::RelocalizeErrors { locale }) => {
                            self.handle_relocalize_errors(locale);
                        }
                        Some(UploadCommand::AbortAll { ack }) => {
                            self.handle_abort_all(ack);
                        }
                        None => {
                            // Manager handle dropped: cancel remaining workers and exit.
                            self.handle_abort_all(None);
                            while self.join_set.join_next().await.is_some() {}
                            break;
                        }
                    }
                }
                Some(update) = worker_rx.recv() => {
                    self.handle_worker_update(update);
                }
                Some(outcome) = self.join_set.join_next(), if has_workers => {
                    self.handle_worker_finished(outcome);
                }
                _ = async {
                    if let Some(deadline) = next_dismiss {
                        tokio::time::sleep_until(deadline).await;
                    }
                }, if next_dismiss.is_some() => {
                    self.flush_expired_auto_dismiss();
                }
                _ = async {
                    if let Some(deadline) = next_progress {
                        tokio::time::sleep_until(deadline).await;
                    }
                }, if next_progress.is_some() => {
                    if self.progress_dirty {
                        self.publish_snapshot(true);
                    }
                }
            }
        }
    }
}

fn is_deterministic_upload_error(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::BookFileUnsupported
            | ErrorCode::BookFileInvalidHashFormat
            | ErrorCode::BookFileHashMismatch
            | ErrorCode::BookFileSizeMismatch
            | ErrorCode::ImportFileTooLarge
            | ErrorCode::CommonPayloadTooLarge
    )
}

async fn run_upload_worker(
    task_id: u64,
    item: EnqueueUploadItem,
    target: ImportTarget,
    opener: Arc<dyn UploadSourceOpener>,
    flight_map: Arc<ClientHashFlightMap>,
    cancel: CancellationToken,
    worker_tx: mpsc::UnboundedSender<WorkerUpdate>,
) -> Result<GetBookOutput, ErrorCode> {
    if cancel.is_cancelled() {
        return Err(ErrorCode::ImportCancelled);
    }

    let is_cloud = target.is_cloud();
    let _ = derive_book_title(&item.file_name)?;

    let (reader, opened_size) = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
        res = opener.open_source(&item.locator) => res?,
    };
    let effective_total = item.total_bytes.or(opened_size);

    let _ = worker_tx.send(WorkerUpdate::Stage {
        task_id,
        is_cloud,
        stage: UploadActiveStage::Staging,
        total_bytes: effective_total,
    });

    let mut staged = {
        let worker_tx_ref = &worker_tx;
        let mut stage_cb = |bytes: u64| {
            let _ = worker_tx_ref.send(WorkerUpdate::Progress {
                task_id,
                stage: UploadActiveStage::Staging,
                bytes_processed: bytes,
                total_bytes: effective_total,
            });
        };
        stage_book(
            &target.staging_temp_dir(),
            reader,
            target.size_limit(),
            &cancel,
            Some(&mut stage_cb as ImportProgressCallback<'_>),
        )
        .await?
    };

    let title = resolve_staged_book_title(&item.file_name, &mut staged).await?;
    let StagedBook {
        mut file,
        hash,
        size,
    } = staged;

    let store_stage = if is_cloud {
        UploadActiveStage::Uploading
    } else {
        UploadActiveStage::Finalizing
    };

    // Single-flight deduplication per content hash within this client process.
    // Acquire/join BEFORE probing so sibling tasks join the in-flight leader instead of hitting
    // the leader's own 409 upload lock or issuing redundant probe requests.
    loop {
        if cancel.is_cancelled() {
            return Err(ErrorCode::ImportCancelled);
        }

        match flight_map.acquire_or_join(&hash, size) {
            FlightRole::Leader(leader_guard) => {
                let probe_res = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(ErrorCode::ImportCancelled),
                    res = target.probe_file(&hash) => res,
                };
                match probe_res {
                    Ok(FileProbeStatus::Exists) => {
                        leader_guard.finish(Ok(()));
                        break;
                    }
                    Ok(FileProbeStatus::Missing) => {}
                    Err(code) => {
                        leader_guard.finish(Err(code));
                        return Err(code);
                    }
                }

                let _ = worker_tx.send(WorkerUpdate::Stage {
                    task_id,
                    is_cloud,
                    stage: store_stage,
                    total_bytes: Some(size),
                });

                if file.seek(SeekFrom::Start(0)).await.is_err() {
                    leader_guard.finish(Err(ErrorCode::CommonInternalServerError));
                    return Err(ErrorCode::CommonInternalServerError);
                }
                let attempt_file = match file.try_clone().await {
                    Ok(cloned) => cloned,
                    Err(_) => {
                        leader_guard.finish(Err(ErrorCode::CommonInternalServerError));
                        return Err(ErrorCode::CommonInternalServerError);
                    }
                };

                let store_res = {
                    let worker_tx_ref = &worker_tx;
                    let leader_ref = &leader_guard;
                    let mut store_cb = |bytes: u64| {
                        leader_ref.update_progress(bytes, size);
                        let _ = worker_tx_ref.send(WorkerUpdate::Progress {
                            task_id,
                            stage: store_stage,
                            bytes_processed: bytes,
                            total_bytes: Some(size),
                        });
                    };
                    target
                        .store_staged_file(
                            &hash,
                            size,
                            &item.file_name,
                            attempt_file,
                            &cancel,
                            Some(&mut store_cb as ImportProgressCallback<'_>),
                        )
                        .await
                };
                leader_guard.finish(store_res);
                store_res?;
                break;
            }
            FlightRole::Follower(mut rx) => {
                let _ = worker_tx.send(WorkerUpdate::Stage {
                    task_id,
                    is_cloud,
                    stage: store_stage,
                    total_bytes: Some(size),
                });
                let outcome = loop {
                    let state = rx.borrow_and_update().clone();
                    match state {
                        FlightState::Uploading {
                            bytes_sent,
                            total_bytes,
                        } => {
                            let _ = worker_tx.send(WorkerUpdate::Progress {
                                task_id,
                                stage: store_stage,
                                bytes_processed: bytes_sent,
                                total_bytes: Some(total_bytes),
                            });
                        }
                        FlightState::Finished(res) => break Some(res),
                    }
                    let changed = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(ErrorCode::ImportCancelled),
                        res = rx.changed() => res,
                    };
                    if changed.is_err() {
                        break None;
                    }
                };
                match outcome {
                    Some(Ok(())) => break,
                    Some(Err(code))
                        if is_session_invalid_code(code) || is_deterministic_upload_error(code) =>
                    {
                        return Err(code);
                    }
                    Some(Err(_)) | None => continue,
                }
            }
        }
    }

    if cancel.is_cancelled() {
        return Err(ErrorCode::ImportCancelled);
    }

    if is_cloud {
        let _ = worker_tx.send(WorkerUpdate::Stage {
            task_id,
            is_cloud,
            stage: UploadActiveStage::Finalizing,
            total_bytes: Some(size),
        });
    }

    target.create_book(title, hash).await
}

pub struct UploadManager {
    cmd_tx: mpsc::UnboundedSender<UploadCommand>,
    latest_snapshot: Arc<RwLock<UploadPanelSnapshotDto>>,
    can_import_books: bool,
    resolve_target: TargetResolver,
    error_dispatcher: Arc<ErrorDispatcher>,
}

impl UploadManager {
    pub(crate) fn new(
        initial_concurrency: usize,
        initial_locale: AppLocale,
        can_import_books: bool,
        retain_completed_uploads: bool,
        error_dispatcher: Arc<ErrorDispatcher>,
        source_opener: Arc<dyn UploadSourceOpener>,
        resolve_target: TargetResolver,
    ) -> Self {
        let concurrency = clamp_upload_concurrency(initial_concurrency);
        let latest_snapshot = Arc::new(RwLock::new(initial_snapshot(concurrency)));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (worker_tx, worker_rx) = mpsc::unbounded_channel();

        let coordinator = CoordinatorState {
            tasks: BTreeMap::new(),
            pending_queue: VecDeque::new(),
            active_workers: HashMap::new(),
            cancelling_workers: HashSet::new(),
            join_id_to_task: HashMap::new(),
            auto_dismiss_deadlines: HashMap::new(),
            join_set: JoinSet::new(),
            max_concurrency: concurrency,
            next_task_id: 1,
            revision: 0,
            visible: false,
            completed_counter: 0,
            session_total: 0,
            session_succeeded: 0,
            session_failed: 0,
            locale: initial_locale,
            last_progress_emit: None,
            progress_dirty: false,
            drain_waiters: Vec::new(),
            flight_map: Arc::new(ClientHashFlightMap::new()),
            source_opener,
            resolve_target: Arc::clone(&resolve_target),
            error_dispatcher: Arc::clone(&error_dispatcher),
            latest_snapshot: Arc::clone(&latest_snapshot),
            worker_tx,
            auto_dismiss_delay: AUTO_DISMISS_DELAY,
            retain_completed_uploads,
        };

        tokio::spawn(coordinator.run(cmd_rx, worker_rx));

        Self {
            cmd_tx,
            latest_snapshot,
            can_import_books,
            resolve_target,
            error_dispatcher,
        }
    }

    pub fn snapshot(&self) -> UploadPanelSnapshotDto {
        self.latest_snapshot
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn ensure_can_enqueue(&self) -> Result<(), CommandErrorPayload> {
        if !self.can_import_books {
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ImportNotSupportedOnPlatform));
        }
        if let Err(code) = (self.resolve_target)() {
            return Err(self.error_dispatcher.dispatch_operation(code));
        }
        Ok(())
    }

    pub async fn enqueue(
        &self,
        items: Vec<EnqueueUploadItem>,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.ensure_can_enqueue()?;
        if items.is_empty() {
            return Ok(self.snapshot());
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UploadCommand::Enqueue {
            items,
            reply: reply_tx,
        });
        Ok(reply_rx.await.unwrap_or_else(|_| self.snapshot()))
    }

    pub async fn enqueue_paths(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.ensure_can_enqueue()?;
        if paths.is_empty() {
            return Ok(self.snapshot());
        }
        let expanded = tokio::task::spawn_blocking(move || expand_upload_paths(paths))
            .await
            .map_err(|_| {
                self.error_dispatcher
                    .dispatch_operation(ErrorCode::CommonInternalServerError)
            })?;

        if expanded.items.is_empty() {
            if expanded.had_directory {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::ImportNoSupportedBooksFound));
            }
            return Ok(self.snapshot());
        }

        self.enqueue(expanded.items).await
    }

    pub async fn retry_task(
        &self,
        task_id: u64,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.ensure_can_enqueue()?;
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UploadCommand::RetryTask {
            task_id,
            reply: reply_tx,
        });
        Ok(reply_rx.await.unwrap_or_else(|_| self.snapshot()))
    }

    pub async fn cancel_task(&self, task_id: u64) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UploadCommand::CancelTask {
            task_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub async fn cancel_all_remaining(&self) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UploadCommand::CancelAllRemaining { reply: reply_tx });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub async fn dismiss_task(&self, task_id: u64) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UploadCommand::DismissTask {
            task_id,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub async fn dismiss_failed(&self) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UploadCommand::DismissFailed { reply: reply_tx });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub async fn clear_completed(&self) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UploadCommand::ClearCompleted { reply: reply_tx });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub async fn set_concurrency(&self, concurrency: usize) -> UploadPanelSnapshotDto {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UploadCommand::SetConcurrency {
            concurrency,
            reply: reply_tx,
        });
        reply_rx.await.unwrap_or_else(|_| self.snapshot())
    }

    pub fn relocalize_errors(&self, locale: AppLocale) {
        let _ = self
            .cmd_tx
            .send(UploadCommand::RelocalizeErrors { locale });
    }

    /// Synchronous non-blocking abort safe to call from inside a worker's
    /// `on_session_invalidated` callback without deadlocking.
    pub fn abort_all(&self) {
        let _ = self.cmd_tx.send(UploadCommand::AbortAll { ack: None });
    }

    /// Cancels all queued and active uploads and awaits full worker teardown so
    /// all `Arc<LocalLibrary>` clones and temporary file handles are released.
    pub async fn abort_all_and_drain(&self) {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(UploadCommand::AbortAll { ack: Some(ack_tx) })
            .is_ok()
        {
            let _ = ack_rx.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_upload_paths_skips_hidden_children_preserves_dot_root_and_sorts_stably() {
        let tmp = tempfile::tempdir().unwrap();
        // Root path itself contains a dot-prefixed directory segment (simulating ~/.local or /tmp/.tmp*)
        let dot_root = tmp.path().join(".hidden-root");
        fs::create_dir_all(dot_root.join("b_sub")).unwrap();
        fs::create_dir_all(dot_root.join("a_sub")).unwrap();
        fs::create_dir_all(dot_root.join(".git")).unwrap();

        fs::write(dot_root.join("b_sub/two.PDF"), b"pdf").unwrap();
        fs::write(dot_root.join("a_sub/one.epub"), b"epub").unwrap();
        fs::write(dot_root.join("a_sub/ignored.txt"), b"txt").unwrap();
        fs::write(dot_root.join(".git/secret.epub"), b"hidden").unwrap();
        fs::write(dot_root.join(".dotfile.pdf"), b"hidden-file").unwrap();

        let expanded = expand_upload_paths(vec![dot_root.clone()]);
        assert!(expanded.had_directory);
        let names: Vec<&str> = expanded.items.iter().map(|i| i.file_name.as_str()).collect();
        assert_eq!(names, vec!["one.epub", "two.PDF"]);

        // Dropping an unsupported single file still enqueues it so the panel displays the error.
        let txt = tmp.path().join("notes.txt");
        fs::write(&txt, b"plain text").unwrap();
        let single = expand_upload_paths(vec![txt]);
        assert!(!single.had_directory);
        assert_eq!(single.items.len(), 1);
        assert_eq!(single.items[0].file_name, "notes.txt");

        // Empty directory sets had_directory = true with empty items.
        let empty_dir = tmp.path().join("empty_folder");
        fs::create_dir_all(&empty_dir).unwrap();
        fs::write(empty_dir.join("readme.md"), b"not a book").unwrap();
        let empty_res = expand_upload_paths(vec![empty_dir]);
        assert!(empty_res.had_directory);
        assert!(empty_res.items.is_empty());
    }

    #[test]
    fn expand_upload_paths_deduplicates_overlapping_directories_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("library");
        let sub = parent.join("sub");
        fs::create_dir_all(&sub).unwrap();

        let root_book = parent.join("root.epub");
        let sub_book = sub.join("book.epub");
        fs::write(&root_book, b"epub-root").unwrap();
        fs::write(&sub_book, b"epub-sub").unwrap();

        let parent_then_sub = expand_upload_paths(vec![parent.clone(), sub.clone()]);
        assert_eq!(parent_then_sub.items.len(), 2);
        let mut names_ps: Vec<&str> = parent_then_sub
            .items
            .iter()
            .map(|i| i.file_name.as_str())
            .collect();
        names_ps.sort_unstable();
        assert_eq!(names_ps, vec!["book.epub", "root.epub"]);

        let sub_then_parent = expand_upload_paths(vec![sub.clone(), parent.clone()]);
        assert_eq!(sub_then_parent.items.len(), 2);
        let mut names_sp: Vec<&str> = sub_then_parent
            .items
            .iter()
            .map(|i| i.file_name.as_str())
            .collect();
        names_sp.sort_unstable();
        assert_eq!(names_sp, vec!["book.epub", "root.epub"]);

        let dup_files = expand_upload_paths(vec![sub_book.clone(), sub_book.clone()]);
        assert!(!dup_files.had_directory);
        assert_eq!(dup_files.items.len(), 1);
        assert_eq!(dup_files.items[0].file_name, "book.epub");

        let dir_and_nested_file = expand_upload_paths(vec![parent.clone(), sub_book.clone()]);
        assert_eq!(dir_and_nested_file.items.len(), 2);

        let nested_file_and_dir = expand_upload_paths(vec![sub_book, parent]);
        assert_eq!(nested_file_and_dir.items.len(), 2);
    }
}
