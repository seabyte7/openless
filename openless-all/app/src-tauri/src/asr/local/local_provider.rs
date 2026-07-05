//! 本地 Qwen3-ASR 在 dictation 路径上的适配器。
//!
//! 与 `WhisperBatchASR` 形状对齐：实现 `AudioConsumer` 缓冲 PCM，stop 时
//! 产出 `RawTranscript`。`BatchOnly` 保持现有整段伪流式路径；`DictationLive`
//! 是后续普通听写 live-first 接入用的可测试状态机，本 issue 先不在生产路径启用。
//!
//! engine 由 `LocalAsrCache` 提供——Coordinator 在 build_local_qwen3 里
//! 取已缓存的引擎再传进来，避免每次会话都重加载 1.2GB+ 模型。

#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "macos")]
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::thread::{self, JoinHandle};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use parking_lot::Mutex;
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Emitter};
#[cfg(target_os = "macos")]
use uuid::Uuid;

#[cfg(target_os = "macos")]
use super::{LocalAsrCacheOutcome, QwenAsrEngine};
#[cfg(target_os = "macos")]
use crate::asr::RawTranscript;

#[cfg(target_os = "macos")]
use super::qwen_engine::QwenLiveAudioSource;

#[cfg(target_os = "macos")]
const SAMPLE_RATE_HZ: u64 = 16_000;
#[cfg(target_os = "macos")]
const BYTES_PER_SAMPLE: u64 = 2;
#[cfg(target_os = "macos")]
const BATCH_STREAM_TAIL_SILENCE_SAMPLES: usize = 8_000;
#[cfg(target_os = "macos")]
const LIVE_START_THRESHOLD_MS: u64 = 2_000;
#[cfg(target_os = "macos")]
const LIVE_FEEDER_CHANNEL_CAPACITY: usize = 64;
#[cfg(target_os = "macos")]
const LIVE_CANCEL_JOIN_GRACE_MS: u64 = 1_000;
#[cfg(target_os = "macos")]
const LIVE_PROGRESS_STALL_MS: u64 = 3_000;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalQwenSessionMode {
    #[allow(dead_code)]
    DictationLive {
        session_id: Uuid,
    },
    BatchOnly,
}

#[cfg(target_os = "macos")]
impl Default for LocalQwenSessionMode {
    fn default() -> Self {
        Self::BatchOnly
    }
}

#[cfg(target_os = "macos")]
impl LocalQwenSessionMode {
    fn live_session_id(self) -> Option<Uuid> {
        match self {
            Self::DictationLive { session_id } => Some(session_id),
            Self::BatchOnly => None,
        }
    }
}

#[cfg(target_os = "macos")]
pub struct LocalQwenAsr {
    engine: Arc<QwenAsrEngine>,
    model_id: String,
    #[allow(dead_code)]
    model_dir: PathBuf,
    engine_cache: LocalAsrCacheOutcome,
    mode: LocalQwenSessionMode,
    /// 16-bit LE PCM 字节缓冲（recorder 推什么我们存什么），live 和 fallback
    /// 都必须以它为可靠来源。一次会话最多几 MB，stop 时 clone 一次可接受。
    buffer: Mutex<Vec<u8>>,
    live: Mutex<LocalQwenLiveController>,
    app: AppHandle,
}

#[cfg(target_os = "macos")]
impl LocalQwenAsr {
    #[allow(dead_code)]
    pub fn new(
        app: AppHandle,
        engine: Arc<QwenAsrEngine>,
        model_id: String,
        model_dir: PathBuf,
        engine_cache: LocalAsrCacheOutcome,
    ) -> Self {
        Self::new_with_mode(
            app,
            engine,
            model_id,
            model_dir,
            engine_cache,
            LocalQwenSessionMode::BatchOnly,
        )
    }

    pub fn new_with_mode(
        app: AppHandle,
        engine: Arc<QwenAsrEngine>,
        model_id: String,
        model_dir: PathBuf,
        engine_cache: LocalAsrCacheOutcome,
        mode: LocalQwenSessionMode,
    ) -> Self {
        Self {
            engine,
            model_id,
            model_dir,
            engine_cache,
            mode,
            buffer: Mutex::new(Vec::new()),
            live: Mutex::new(LocalQwenLiveController::default()),
            app,
        }
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    #[allow(dead_code)]
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    pub fn engine_cache(&self) -> LocalAsrCacheOutcome {
        self.engine_cache
    }

    pub fn path_label(&self) -> &'static str {
        if self.mode.live_session_id().is_some() {
            "live"
        } else {
            "batch_stream"
        }
    }

    /// 当前缓冲音频时长（毫秒）。Coordinator 在 transcribe() 调用前读取，
    /// 用来给本地 Qwen ASR 计算动态超时。不消费缓冲。
    pub fn buffer_duration_ms(&self) -> u64 {
        duration_ms_from_pcm_bytes(self.buffer.lock().len())
    }

    /// stop 时调用：`BatchOnly` 保持当前整段伪流式行为；`DictationLive`
    /// 在录音期间已启动 live worker 时只负责 finish + join。
    pub async fn transcribe(self: Arc<Self>) -> Result<RawTranscript> {
        let pcm_bytes = self.buffer.lock().clone();
        if pcm_bytes.is_empty() {
            return Ok(RawTranscript {
                text: String::new(),
                duration_ms: 0,
            });
        }
        let duration_ms = duration_ms_from_pcm_bytes(pcm_bytes.len());

        if self.mode.live_session_id().is_none() {
            return self.transcribe_batch_stream(pcm_bytes, duration_ms).await;
        }

        self.transcribe_live_mode(pcm_bytes, duration_ms).await
    }

    pub fn cancel(&self) {
        self.buffer.lock().clear();
        self.live.lock().cancel();
    }

    async fn transcribe_batch_stream(
        &self,
        pcm_bytes: Vec<u8>,
        duration_ms: u64,
    ) -> Result<RawTranscript> {
        let mut samples_f32 = i16_le_bytes_to_f32(&pcm_bytes);
        // `transcribe_stream` 内部按 2s chunk 切片；末 chunk < 2s 且缓冲没有
        // 静默尾巴时，C 引擎不会把它当作"语音已结束"，该 chunk 的转写结果
        // 会被丢弃，导致末段内容消失。这里追加 0.5s 静默（@16kHz = 8000 个
        // f32 零值）作为收尾信号。`duration_ms` 仍按原始缓冲长度计算。
        samples_f32.extend(std::iter::repeat(0.0f32).take(BATCH_STREAM_TAIL_SILENCE_SAMPLES));

        // 注册 token 回调：每个稳定 token 抛 `local-asr-token` 事件。
        // capsule 前端按 sessionId 累积显示。
        let app = self.app.clone();
        let engine = Arc::clone(&self.engine);
        let text = tauri::async_runtime::spawn_blocking(move || {
            engine.transcribe_stream_with_handler(&samples_f32, move |piece: &str| {
                if let Err(e) = app.emit("local-asr-token", piece.to_string()) {
                    log::warn!("[local-asr] emit token failed: {e}");
                }
            })
        })
        .await
        .context("transcribe spawn_blocking join 失败")?
        .context("qwen_transcribe_stream 失败")?;

        self.buffer.lock().clear();
        Ok(RawTranscript { text, duration_ms })
    }

    async fn transcribe_live_mode(
        &self,
        pcm_bytes: Vec<u8>,
        duration_ms: u64,
    ) -> Result<RawTranscript> {
        let audio_secs = duration_ms as f64 / 1000.0;
        let stop_action = {
            let mut live = self.live.lock();
            live.machine.on_stop()
        };
        match stop_action {
            LocalQwenStopAction::DirectFinal => {
                let text = self.transcribe_direct_final(pcm_bytes).await?;
                self.buffer.lock().clear();
                Ok(RawTranscript { text, duration_ms })
            }
            LocalQwenStopAction::FinishLive => {
                let runtime = self.live.lock().take_runtime_for_finalize();
                let Some(runtime) = runtime else {
                    return self
                        .transcribe_fallback_final(
                            pcm_bytes,
                            duration_ms,
                            LocalQwenLiveFallbackReason::WorkerJoinError,
                            LocalQwenFallbackEngine::Cached,
                        )
                        .await;
                };
                let finish_outcome = tauri::async_runtime::spawn_blocking(move || {
                    finalize_live_runtime(runtime, audio_secs)
                })
                .await
                .context("qwen live finalize spawn_blocking join 失败")?;

                match finish_outcome {
                    LocalQwenLiveFinishOutcome::Final(text) => {
                        self.live.lock().machine.on_live_final();
                        self.buffer.lock().clear();
                        Ok(RawTranscript { text, duration_ms })
                    }
                    LocalQwenLiveFinishOutcome::Fallback {
                        reason,
                        detail,
                        engine,
                    } => {
                        log::warn!(
                            "[local-asr fast] provider=local-qwen3 model={} path=live event=fallback_needed reason={} fallback_engine={} detail={}",
                            self.model_id,
                            reason.as_str(),
                            engine.as_str(),
                            detail
                        );
                        self.transcribe_fallback_final(pcm_bytes, duration_ms, reason, engine)
                            .await
                    }
                }
            }
            LocalQwenStopAction::FallbackNeeded(reason) => {
                let runtime = self.live.lock().take_runtime_for_cancel();
                let (reason, engine) = if let Some(runtime) = runtime {
                    let cancel_reason = reason;
                    tauri::async_runtime::spawn_blocking(move || {
                        cancel_live_runtime_for_fallback(runtime, cancel_reason)
                    })
                    .await
                    .context("qwen live cancel spawn_blocking join 失败")?
                } else {
                    (reason, LocalQwenFallbackEngine::Cached)
                };
                self.transcribe_fallback_final(pcm_bytes, duration_ms, reason, engine)
                    .await
            }
            LocalQwenStopAction::Cancelled => {
                anyhow::bail!("local Qwen live session cancelled");
            }
        }
    }

    async fn transcribe_direct_final(&self, pcm_bytes: Vec<u8>) -> Result<String> {
        let samples_f32 = i16_le_bytes_to_f32(&pcm_bytes);
        let engine = Arc::clone(&self.engine);
        tauri::async_runtime::spawn_blocking(move || engine.transcribe_stream_final(&samples_f32))
            .await
            .context("qwen direct final spawn_blocking join 失败")?
            .context("qwen direct final failed")
    }

    async fn transcribe_fallback_final(
        &self,
        pcm_bytes: Vec<u8>,
        duration_ms: u64,
        reason: LocalQwenLiveFallbackReason,
        engine: LocalQwenFallbackEngine,
    ) -> Result<RawTranscript> {
        let model_id = self.model_id.clone();
        let model_dir = self.model_dir.clone();
        let fallback_started = Instant::now();
        let samples_f32 = i16_le_bytes_to_f32(&pcm_bytes);
        let engine_for_log = engine.as_str();
        let reason_for_log = reason.as_str();

        let text_result = match engine {
            LocalQwenFallbackEngine::Cached => {
                let engine = Arc::clone(&self.engine);
                tauri::async_runtime::spawn_blocking(move || {
                    engine.transcribe_stream_final(&samples_f32)
                })
                .await
                .context("qwen cached fallback spawn_blocking join 失败")?
            }
            LocalQwenFallbackEngine::Fresh => tauri::async_runtime::spawn_blocking(move || {
                let engine = QwenAsrEngine::load(&model_dir)
                    .context("load fresh qwen engine for fallback")?;
                engine.transcribe_stream_final(&samples_f32)
            })
            .await
            .context("qwen fresh fallback spawn_blocking join 失败")?,
        };

        let fallback_asr_ms = fallback_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;

        match validate_fallback_text(text_result, reason, engine) {
            Ok(text) => {
                log::info!(
                    "[local-asr fast] provider=local-qwen3 model={} path=fallback audio_ms={} fallback={} fallback_engine={} fallback_asr_ms={} status=ok",
                    model_id,
                    duration_ms,
                    reason_for_log,
                    engine_for_log,
                    fallback_asr_ms
                );
                self.buffer.lock().clear();
                Ok(RawTranscript { text, duration_ms })
            }
            Err(error) => {
                log::error!(
                    "[local-asr fast] provider=local-qwen3 model={} path=fallback audio_ms={} fallback={} fallback_engine={} fallback_asr_ms={} status=error error={error:#}",
                    model_id,
                    duration_ms,
                    reason_for_log,
                    engine_for_log,
                    fallback_asr_ms
                );
                Err(error)
            }
        }
    }

    fn consume_pcm_chunk_live(&self, pcm: &[u8]) {
        let live_action = {
            let mut buffer = self.buffer.lock();
            buffer.extend_from_slice(pcm);

            let mut live = self.live.lock();
            match live.machine.on_pcm_chunk(pcm.len()) {
                LocalQwenConsumeAction::StartLive => {
                    LocalQwenRuntimeAction::StartLive(buffer.clone())
                }
                LocalQwenConsumeAction::FeedLive => {
                    if let Some(tx) = live.feeder_tx() {
                        LocalQwenRuntimeAction::FeedLive(tx, pcm.to_vec())
                    } else {
                        live.machine
                            .on_unhealthy(LocalQwenLiveFallbackReason::WorkerJoinError);
                        LocalQwenRuntimeAction::None
                    }
                }
                LocalQwenConsumeAction::None => LocalQwenRuntimeAction::None,
            }
        };

        match live_action {
            LocalQwenRuntimeAction::StartLive(initial_pcm) => {
                self.start_live_from_pcm(initial_pcm);
            }
            LocalQwenRuntimeAction::FeedLive(tx, chunk) => match tx.try_send(chunk) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    self.mark_live_fallback_needed(LocalQwenLiveFallbackReason::FeederOverflow);
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.mark_live_fallback_needed(LocalQwenLiveFallbackReason::WorkerJoinError);
                }
            },
            LocalQwenRuntimeAction::None => {}
        }
    }

    fn start_live_from_pcm(&self, initial_pcm: Vec<u8>) {
        let Some(session_id) = self.mode.live_session_id() else {
            return;
        };

        let source = match QwenLiveAudioSource::create() {
            Ok(source) => Arc::new(source),
            Err(error) => {
                log::warn!(
                    "[local-asr fast] session={} path=live event=start_failed reason=create_source error={error:#}",
                    session_id
                );
                self.mark_live_start_failed();
                return;
            }
        };

        let (feeder_tx, feeder_rx) = sync_channel::<Vec<u8>>(LIVE_FEEDER_CHANNEL_CAPACITY);
        let feeder_source = Arc::clone(&source);
        let feeder_handle = match thread::Builder::new()
            .name("openless-qwen-live-feeder".into())
            .spawn(move || -> Result<()> {
                while let Ok(chunk) = feeder_rx.recv() {
                    feeder_source
                        .append_s16le(&chunk)
                        .context("qwen live feeder append_s16le failed")?;
                }
                Ok(())
            }) {
            Ok(handle) => handle,
            Err(error) => {
                log::warn!(
                    "[local-asr fast] session={} path=live event=start_failed reason=spawn_feeder error={error}",
                    session_id
                );
                source.cancel();
                self.mark_live_start_failed();
                return;
            }
        };

        if let Err(error) = feeder_tx.try_send(initial_pcm) {
            log::warn!(
                "[local-asr fast] session={} path=live event=start_failed reason=initial_feed error={error}",
                session_id
            );
            source.cancel();
            drop(feeder_tx);
            let _ = feeder_handle.join();
            self.mark_live_fallback_needed(LocalQwenLiveFallbackReason::FeederOverflow);
            return;
        }

        let worker_engine = Arc::clone(&self.engine);
        let worker_source = Arc::clone(&source);
        let app = self.app.clone();
        let (worker_result_tx, worker_result_rx) =
            sync_channel::<std::result::Result<String, LocalQwenLiveWorkerFailure>>(1);
        let started_at = Instant::now();
        let last_progress_ms = Arc::new(AtomicU64::new(0));
        let worker_progress_ms = Arc::clone(&last_progress_ms);
        let worker_handle = match thread::Builder::new()
            .name("openless-qwen-live-worker".into())
            .spawn(move || {
                let worker_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_engine.transcribe_stream_live_with_handler(
                        &worker_source,
                        move |piece| {
                            let elapsed_ms =
                                started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                            worker_progress_ms.store(elapsed_ms, Ordering::Relaxed);
                            if let Err(e) = app.emit("local-asr-token", piece.to_string()) {
                                log::warn!("[local-asr] emit live token failed: {e}");
                            }
                        },
                    )
                }));
                let message = match worker_result {
                    Ok(Ok(text)) => Ok(text),
                    Ok(Err(error)) => Err(LocalQwenLiveWorkerFailure::from_error(error)),
                    Err(_) => Err(LocalQwenLiveWorkerFailure::new(
                        LocalQwenLiveFallbackReason::WorkerPanic,
                        "qwen live worker panicked",
                    )),
                };
                let _ = worker_result_tx.send(message);
            }) {
            Ok(handle) => handle,
            Err(error) => {
                log::warn!(
                    "[local-asr fast] session={} path=live event=start_failed reason=spawn_worker error={error}",
                    session_id
                );
                source.cancel();
                drop(feeder_tx);
                let _ = feeder_handle.join();
                self.mark_live_start_failed();
                return;
            }
        };

        let mut live = self.live.lock();
        live.runtime = Some(LocalQwenLiveRuntime {
            source,
            feeder_tx: Some(feeder_tx),
            feeder_handle: Some(feeder_handle),
            worker_handle: Some(worker_handle),
            worker_result_rx,
            started_at,
            last_progress_ms,
        });
        live.machine.on_live_worker_started();
    }

    fn mark_live_fallback_needed(&self, reason: LocalQwenLiveFallbackReason) {
        self.live.lock().machine.on_unhealthy(reason);
    }

    fn mark_live_start_failed(&self) {
        self.live.lock().machine.on_live_start_failed();
    }
}

#[cfg(target_os = "macos")]
impl crate::recorder::AudioConsumer for LocalQwenAsr {
    fn consume_pcm_chunk(&self, pcm: &[u8]) {
        if self.mode.live_session_id().is_some() {
            self.consume_pcm_chunk_live(pcm);
            return;
        }
        self.buffer.lock().extend_from_slice(pcm);
    }
}

#[cfg(target_os = "macos")]
#[derive(Default)]
struct LocalQwenLiveController {
    machine: LocalQwenLiveStateMachine,
    runtime: Option<LocalQwenLiveRuntime>,
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveController {
    fn feeder_tx(&self) -> Option<SyncSender<Vec<u8>>> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.feeder_tx.as_ref())
            .cloned()
    }

    fn take_runtime_for_finalize(&mut self) -> Option<LocalQwenLiveRuntime> {
        self.runtime.take()
    }

    fn take_runtime_for_cancel(&mut self) -> Option<LocalQwenLiveRuntime> {
        self.runtime.take()
    }

    fn cancel(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.cancel_detached();
        }
        self.machine.cancel();
    }
}

#[cfg(target_os = "macos")]
struct LocalQwenLiveRuntime {
    source: Arc<QwenLiveAudioSource>,
    feeder_tx: Option<SyncSender<Vec<u8>>>,
    feeder_handle: Option<JoinHandle<Result<()>>>,
    worker_handle: Option<JoinHandle<()>>,
    worker_result_rx: Receiver<std::result::Result<String, LocalQwenLiveWorkerFailure>>,
    started_at: Instant,
    last_progress_ms: Arc<AtomicU64>,
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveRuntime {
    fn cancel_detached(mut self) {
        self.feeder_tx.take();
        self.source.cancel();
        self.feeder_handle.take();
        self.worker_handle.take();
    }
}

#[cfg(target_os = "macos")]
fn finalize_live_runtime(
    mut runtime: LocalQwenLiveRuntime,
    audio_secs: f64,
) -> LocalQwenLiveFinishOutcome {
    runtime.feeder_tx.take();
    if let Some(handle) = runtime.feeder_handle.take() {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return fallback_after_live_cancel_with_detail(
                    runtime,
                    LocalQwenLiveFallbackReason::FeederAppendFailed,
                    format!("qwen live feeder failed: {error:#}"),
                );
            }
            Err(_) => {
                return fallback_after_live_cancel_with_detail(
                    runtime,
                    LocalQwenLiveFallbackReason::FeederAppendFailed,
                    "qwen live feeder panicked",
                );
            }
        }
    }

    runtime.source.finish();
    let finalize_timeout = live_finalize_timeout(audio_secs);
    let progress_stall_timeout = live_progress_stall_timeout(finalize_timeout);
    let finalize_started_ms = runtime
        .started_at
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let deadline = Instant::now() + finalize_timeout;

    loop {
        let now = Instant::now();
        if now >= deadline {
            return fallback_after_live_cancel(
                runtime,
                LocalQwenLiveFallbackReason::FinalizeTimeout,
            );
        }
        let remaining = deadline.saturating_duration_since(now);
        let wait_for = remaining.min(Duration::from_millis(100));
        match runtime.worker_result_rx.recv_timeout(wait_for) {
            Ok(Ok(text)) => {
                join_finished_worker(&mut runtime);
                return live_text_finish_outcome(text);
            }
            Ok(Err(failure)) => {
                join_finished_worker(&mut runtime);
                return LocalQwenLiveFinishOutcome::fallback(
                    failure.reason,
                    failure.detail,
                    LocalQwenFallbackEngine::Cached,
                );
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                join_finished_worker(&mut runtime);
                return LocalQwenLiveFinishOutcome::fallback(
                    LocalQwenLiveFallbackReason::WorkerJoinError,
                    "qwen live worker result channel disconnected",
                    LocalQwenFallbackEngine::Cached,
                );
            }
        }
        if live_progress_idle_duration(&runtime, finalize_started_ms) >= progress_stall_timeout {
            return fallback_after_live_cancel(runtime, LocalQwenLiveFallbackReason::ProgressStall);
        }
    }
}

#[cfg(target_os = "macos")]
fn cancel_live_runtime_for_fallback(
    runtime: LocalQwenLiveRuntime,
    reason: LocalQwenLiveFallbackReason,
) -> (LocalQwenLiveFallbackReason, LocalQwenFallbackEngine) {
    match fallback_after_live_cancel(runtime, reason) {
        LocalQwenLiveFinishOutcome::Final(_) => (reason, LocalQwenFallbackEngine::Cached),
        LocalQwenLiveFinishOutcome::Fallback { reason, engine, .. } => (reason, engine),
    }
}

#[cfg(target_os = "macos")]
fn fallback_after_live_cancel(
    runtime: LocalQwenLiveRuntime,
    reason: LocalQwenLiveFallbackReason,
) -> LocalQwenLiveFinishOutcome {
    fallback_after_live_cancel_with_detail(runtime, reason, reason.as_str())
}

#[cfg(target_os = "macos")]
fn fallback_after_live_cancel_with_detail(
    mut runtime: LocalQwenLiveRuntime,
    reason: LocalQwenLiveFallbackReason,
    detail: impl Into<String>,
) -> LocalQwenLiveFinishOutcome {
    let detail = detail.into();
    runtime.feeder_tx.take();
    runtime.source.cancel();
    if let Some(handle) = runtime.feeder_handle.take() {
        let _ = handle.join();
    }

    match runtime
        .worker_result_rx
        .recv_timeout(Duration::from_millis(LIVE_CANCEL_JOIN_GRACE_MS))
    {
        Ok(_) | Err(RecvTimeoutError::Disconnected) => {
            join_finished_worker(&mut runtime);
            LocalQwenLiveFinishOutcome::fallback(
                reason,
                format!("{detail}; live worker exited within cancel grace"),
                LocalQwenFallbackEngine::Cached,
            )
        }
        Err(RecvTimeoutError::Timeout) => {
            runtime.worker_handle.take();
            LocalQwenLiveFinishOutcome::fallback(
                LocalQwenLiveFallbackReason::LiveCancelGraceExceeded,
                format!(
                    "{detail}; {}; live worker still busy after {}ms",
                    reason.as_str(),
                    LIVE_CANCEL_JOIN_GRACE_MS
                ),
                LocalQwenFallbackEngine::Fresh,
            )
        }
    }
}

#[cfg(target_os = "macos")]
fn join_finished_worker(runtime: &mut LocalQwenLiveRuntime) {
    if let Some(handle) = runtime.worker_handle.take() {
        let _ = handle.join();
    }
}

#[cfg(target_os = "macos")]
fn live_progress_idle_duration(
    runtime: &LocalQwenLiveRuntime,
    baseline_elapsed_ms: u64,
) -> Duration {
    let last_progress_ms = runtime.last_progress_ms.load(Ordering::Relaxed);
    let elapsed_ms = runtime
        .started_at
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    Duration::from_millis(elapsed_ms.saturating_sub(last_progress_ms.max(baseline_elapsed_ms)))
}

#[cfg(target_os = "macos")]
fn live_finalize_timeout(audio_secs: f64) -> Duration {
    if audio_secs <= 15.0 {
        Duration::from_secs(3)
    } else if audio_secs <= 60.0 {
        Duration::from_secs(5)
    } else {
        Duration::from_secs((audio_secs * 0.2).ceil() as u64 + 5)
    }
}

#[cfg(target_os = "macos")]
fn live_progress_stall_timeout(finalize_timeout: Duration) -> Duration {
    Duration::from_millis(LIVE_PROGRESS_STALL_MS).min(finalize_timeout)
}

#[cfg(target_os = "macos")]
fn live_text_finish_outcome(text: String) -> LocalQwenLiveFinishOutcome {
    if text.is_empty() {
        return LocalQwenLiveFinishOutcome::fallback(
            LocalQwenLiveFallbackReason::EmptyLiveResult,
            "qwen live returned empty text",
            LocalQwenFallbackEngine::Cached,
        );
    }
    if text.trim().is_empty() {
        return LocalQwenLiveFinishOutcome::fallback(
            LocalQwenLiveFallbackReason::InvalidLiveResult,
            "qwen live returned whitespace-only text",
            LocalQwenFallbackEngine::Cached,
        );
    }
    LocalQwenLiveFinishOutcome::Final(text)
}

#[cfg(target_os = "macos")]
fn validate_fallback_text(
    result: Result<String>,
    live_reason: LocalQwenLiveFallbackReason,
    engine: LocalQwenFallbackEngine,
) -> Result<String> {
    match result {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        Ok(_) => anyhow::bail!(
            "local Qwen fallback returned empty transcript after live_reason={} fallback_engine={}",
            live_reason.as_str(),
            engine.as_str()
        ),
        Err(error) => anyhow::bail!(
            "local Qwen fallback failed after live_reason={} fallback_engine={}: {error:#}",
            live_reason.as_str(),
            engine.as_str()
        ),
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalQwenLivePhase {
    Idle,
    Buffering,
    LiveStarting,
    LiveRunning,
    Finalizing,
    LiveFinal,
    FallbackNeeded,
    Cancelled,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalQwenLiveFallbackReason {
    LiveStartFailed,
    FeederOverflow,
    FeederAppendFailed,
    WorkerPanic,
    WorkerJoinError,
    CNull,
    EmptyLiveResult,
    InvalidLiveResult,
    FinalizeTimeout,
    ProgressStall,
    LiveCancelGraceExceeded,
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveFallbackReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::LiveStartFailed => "live_start_failed",
            Self::FeederOverflow => "feeder_overflow",
            Self::FeederAppendFailed => "feeder_append_failed",
            Self::WorkerPanic => "worker_panic",
            Self::WorkerJoinError => "worker_join_error",
            Self::CNull => "c_null",
            Self::EmptyLiveResult => "empty_live_result",
            Self::InvalidLiveResult => "invalid_live_result",
            Self::FinalizeTimeout => "finalize_timeout",
            Self::ProgressStall => "progress_stall",
            Self::LiveCancelGraceExceeded => "live_cancel_grace_exceeded",
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct LocalQwenLiveWorkerFailure {
    reason: LocalQwenLiveFallbackReason,
    detail: String,
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveWorkerFailure {
    fn new(reason: LocalQwenLiveFallbackReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }

    fn from_error(error: anyhow::Error) -> Self {
        let detail = format!("{error:#}");
        let reason = if detail.contains("NULL") || detail.contains("返回 NULL") {
            LocalQwenLiveFallbackReason::CNull
        } else {
            LocalQwenLiveFallbackReason::WorkerJoinError
        };
        Self { reason, detail }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalQwenFallbackEngine {
    Cached,
    Fresh,
}

#[cfg(target_os = "macos")]
impl LocalQwenFallbackEngine {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cached => "cached",
            Self::Fresh => "fresh",
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum LocalQwenLiveFinishOutcome {
    Final(String),
    Fallback {
        reason: LocalQwenLiveFallbackReason,
        detail: String,
        engine: LocalQwenFallbackEngine,
    },
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveFinishOutcome {
    fn fallback(
        reason: LocalQwenLiveFallbackReason,
        detail: impl Into<String>,
        engine: LocalQwenFallbackEngine,
    ) -> Self {
        Self::Fallback {
            reason,
            detail: detail.into(),
            engine,
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalQwenConsumeAction {
    None,
    StartLive,
    FeedLive,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalQwenStopAction {
    DirectFinal,
    FinishLive,
    FallbackNeeded(LocalQwenLiveFallbackReason),
    Cancelled,
}

#[cfg(target_os = "macos")]
enum LocalQwenRuntimeAction {
    None,
    StartLive(Vec<u8>),
    FeedLive(SyncSender<Vec<u8>>, Vec<u8>),
}

#[cfg(target_os = "macos")]
struct LocalQwenLiveStateMachine {
    phase: LocalQwenLivePhase,
    total_pcm_bytes: usize,
    fallback_reason: Option<LocalQwenLiveFallbackReason>,
}

#[cfg(target_os = "macos")]
impl Default for LocalQwenLiveStateMachine {
    fn default() -> Self {
        Self {
            phase: LocalQwenLivePhase::Idle,
            total_pcm_bytes: 0,
            fallback_reason: None,
        }
    }
}

#[cfg(target_os = "macos")]
impl LocalQwenLiveStateMachine {
    fn on_pcm_chunk(&mut self, n_bytes: usize) -> LocalQwenConsumeAction {
        self.total_pcm_bytes = self.total_pcm_bytes.saturating_add(n_bytes);
        if self.fallback_reason.is_some() {
            return LocalQwenConsumeAction::None;
        }

        match self.phase {
            LocalQwenLivePhase::Idle => {
                self.phase = LocalQwenLivePhase::Buffering;
                if self.buffer_duration_ms() >= LIVE_START_THRESHOLD_MS {
                    self.phase = LocalQwenLivePhase::LiveStarting;
                    LocalQwenConsumeAction::StartLive
                } else {
                    LocalQwenConsumeAction::None
                }
            }
            LocalQwenLivePhase::Buffering => {
                if self.buffer_duration_ms() >= LIVE_START_THRESHOLD_MS {
                    self.phase = LocalQwenLivePhase::LiveStarting;
                    LocalQwenConsumeAction::StartLive
                } else {
                    LocalQwenConsumeAction::None
                }
            }
            LocalQwenLivePhase::LiveRunning => LocalQwenConsumeAction::FeedLive,
            LocalQwenLivePhase::LiveStarting
            | LocalQwenLivePhase::Finalizing
            | LocalQwenLivePhase::LiveFinal
            | LocalQwenLivePhase::FallbackNeeded
            | LocalQwenLivePhase::Cancelled => LocalQwenConsumeAction::None,
        }
    }

    fn on_live_worker_started(&mut self) {
        if self.phase == LocalQwenLivePhase::LiveStarting {
            self.phase = LocalQwenLivePhase::LiveRunning;
        }
    }

    fn on_live_start_failed(&mut self) {
        self.on_unhealthy(LocalQwenLiveFallbackReason::LiveStartFailed);
    }

    fn on_unhealthy(&mut self, reason: LocalQwenLiveFallbackReason) {
        self.fallback_reason = Some(reason);
        if matches!(
            self.phase,
            LocalQwenLivePhase::Idle
                | LocalQwenLivePhase::Buffering
                | LocalQwenLivePhase::LiveStarting
        ) {
            self.phase = LocalQwenLivePhase::FallbackNeeded;
        }
    }

    fn on_stop(&mut self) -> LocalQwenStopAction {
        if self.phase == LocalQwenLivePhase::Cancelled {
            return LocalQwenStopAction::Cancelled;
        }
        if let Some(reason) = self.fallback_reason {
            self.phase = LocalQwenLivePhase::FallbackNeeded;
            return LocalQwenStopAction::FallbackNeeded(reason);
        }

        match self.phase {
            LocalQwenLivePhase::Idle | LocalQwenLivePhase::Buffering => {
                self.phase = LocalQwenLivePhase::Finalizing;
                LocalQwenStopAction::DirectFinal
            }
            LocalQwenLivePhase::LiveStarting | LocalQwenLivePhase::LiveRunning => {
                self.phase = LocalQwenLivePhase::Finalizing;
                LocalQwenStopAction::FinishLive
            }
            LocalQwenLivePhase::FallbackNeeded => LocalQwenStopAction::FallbackNeeded(
                self.fallback_reason
                    .unwrap_or(LocalQwenLiveFallbackReason::WorkerJoinError),
            ),
            LocalQwenLivePhase::Finalizing => {
                LocalQwenStopAction::FallbackNeeded(LocalQwenLiveFallbackReason::WorkerJoinError)
            }
            LocalQwenLivePhase::LiveFinal => LocalQwenStopAction::DirectFinal,
            LocalQwenLivePhase::Cancelled => LocalQwenStopAction::Cancelled,
        }
    }

    fn on_live_final(&mut self) {
        self.phase = LocalQwenLivePhase::LiveFinal;
        self.fallback_reason = None;
    }

    fn cancel(&mut self) {
        self.phase = LocalQwenLivePhase::Cancelled;
        self.fallback_reason = None;
    }

    fn buffer_duration_ms(&self) -> u64 {
        duration_ms_from_pcm_bytes(self.total_pcm_bytes)
    }
}

#[cfg(target_os = "macos")]
fn duration_ms_from_pcm_bytes(bytes: usize) -> u64 {
    (bytes as u64 / BYTES_PER_SAMPLE) * 1000 / SAMPLE_RATE_HZ
}

#[cfg(target_os = "macos")]
fn i16_le_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|c| {
            let v = i16::from_le_bytes([c[0], c[1]]);
            v as f32 / 32768.0
        })
        .collect()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    const ONE_SECOND_PCM_BYTES: usize = 16_000 * 2;

    #[test]
    fn local_qwen_live_state_machine_short_recording_uses_direct_final() {
        let mut machine = LocalQwenLiveStateMachine::default();

        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES),
            LocalQwenConsumeAction::None
        );
        assert_eq!(machine.phase, LocalQwenLivePhase::Buffering);
        assert_eq!(machine.on_stop(), LocalQwenStopAction::DirectFinal);
        assert_eq!(machine.phase, LocalQwenLivePhase::Finalizing);
    }

    #[test]
    fn local_qwen_live_state_machine_fake_worker_reaches_live_final() {
        let mut machine = LocalQwenLiveStateMachine::default();

        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES * 2),
            LocalQwenConsumeAction::StartLive
        );
        assert_eq!(machine.phase, LocalQwenLivePhase::LiveStarting);

        machine.on_live_worker_started();
        assert_eq!(machine.phase, LocalQwenLivePhase::LiveRunning);
        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES),
            LocalQwenConsumeAction::FeedLive
        );

        assert_eq!(machine.on_stop(), LocalQwenStopAction::FinishLive);
        assert_eq!(machine.phase, LocalQwenLivePhase::Finalizing);
        machine.on_live_final();
        assert_eq!(machine.phase, LocalQwenLivePhase::LiveFinal);
    }

    #[test]
    fn local_qwen_live_state_machine_feeder_overflow_preserves_full_pcm() {
        let mut machine = LocalQwenLiveStateMachine::default();
        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES * 2),
            LocalQwenConsumeAction::StartLive
        );
        machine.on_live_worker_started();

        machine.on_unhealthy(LocalQwenLiveFallbackReason::FeederOverflow);
        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES),
            LocalQwenConsumeAction::None
        );
        assert_eq!(machine.total_pcm_bytes, ONE_SECOND_PCM_BYTES * 3);
        assert_eq!(
            machine.on_stop(),
            LocalQwenStopAction::FallbackNeeded(LocalQwenLiveFallbackReason::FeederOverflow)
        );
    }

    #[test]
    fn local_qwen_live_state_machine_start_failure_needs_fallback() {
        let mut machine = LocalQwenLiveStateMachine::default();

        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES * 2),
            LocalQwenConsumeAction::StartLive
        );
        machine.on_live_start_failed();

        assert_eq!(machine.phase, LocalQwenLivePhase::FallbackNeeded);
        assert_eq!(
            machine.on_stop(),
            LocalQwenStopAction::FallbackNeeded(LocalQwenLiveFallbackReason::LiveStartFailed)
        );
    }

    #[test]
    fn local_qwen_live_timeout_policy_scales_by_audio_duration() {
        assert_eq!(live_finalize_timeout(2.1), Duration::from_secs(3));
        assert_eq!(live_finalize_timeout(15.0), Duration::from_secs(3));
        assert_eq!(live_finalize_timeout(30.0), Duration::from_secs(5));
        assert_eq!(live_finalize_timeout(75.0), Duration::from_secs(20));
    }

    #[test]
    fn local_qwen_live_invalid_result_needs_cached_fallback() {
        match live_text_finish_outcome("   ".to_string()) {
            LocalQwenLiveFinishOutcome::Fallback { reason, engine, .. } => {
                assert_eq!(reason, LocalQwenLiveFallbackReason::InvalidLiveResult);
                assert_eq!(engine, LocalQwenFallbackEngine::Cached);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn local_qwen_live_hard_stall_uses_fresh_fallback_engine() {
        let outcome = LocalQwenLiveFinishOutcome::fallback(
            LocalQwenLiveFallbackReason::LiveCancelGraceExceeded,
            "live worker still busy",
            LocalQwenFallbackEngine::Fresh,
        );

        match outcome {
            LocalQwenLiveFinishOutcome::Fallback { reason, engine, .. } => {
                assert_eq!(reason, LocalQwenLiveFallbackReason::LiveCancelGraceExceeded);
                assert_eq!(engine, LocalQwenFallbackEngine::Fresh);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn local_qwen_fallback_both_path_failure_is_clear() {
        let err = validate_fallback_text(
            Err(anyhow::anyhow!("fallback engine failed")),
            LocalQwenLiveFallbackReason::FinalizeTimeout,
            LocalQwenFallbackEngine::Fresh,
        )
        .expect_err("both-path failure should be an error");
        let msg = format!("{err:#}");

        assert!(msg.contains("live_reason=finalize_timeout"));
        assert!(msg.contains("fallback_engine=fresh"));
        assert!(msg.contains("fallback engine failed"));
    }

    #[test]
    fn local_qwen_live_state_machine_cancel_before_live() {
        let mut machine = LocalQwenLiveStateMachine::default();

        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES),
            LocalQwenConsumeAction::None
        );
        machine.cancel();

        assert_eq!(machine.phase, LocalQwenLivePhase::Cancelled);
        assert_eq!(machine.on_stop(), LocalQwenStopAction::Cancelled);
    }

    #[test]
    fn local_qwen_live_state_machine_cancel_while_live() {
        let mut machine = LocalQwenLiveStateMachine::default();

        assert_eq!(
            machine.on_pcm_chunk(ONE_SECOND_PCM_BYTES * 2),
            LocalQwenConsumeAction::StartLive
        );
        machine.on_live_worker_started();
        machine.cancel();

        assert_eq!(machine.phase, LocalQwenLivePhase::Cancelled);
        assert_eq!(machine.on_stop(), LocalQwenStopAction::Cancelled);
    }
}
