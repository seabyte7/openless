//! vendored Open-Less/qwen-asr 的安全 Rust 包装。
//!
//! 当前只暴露**最小可用面**：`load` / `transcribe_audio` / `transcribe_stream`
//! + token 回调。后续接 coordinator 时再扩 prompt/language 设置。

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};

use super::qwen_ffi::{
    qwen_free, qwen_live_audio_append_f32, qwen_live_audio_append_s16le, qwen_live_audio_cancel,
    qwen_live_audio_create, qwen_live_audio_finish, qwen_live_audio_free, qwen_load,
    qwen_set_token_callback, qwen_transcribe_audio, qwen_transcribe_stream,
    qwen_transcribe_stream_live, QwenCtx, QwenLiveAudio,
};

/// FnMut 闭包是 fat pointer，不能直接塞进 `*mut c_void`，所以包一层 Box。
type TokenHandler = dyn FnMut(&str) + Send + 'static;
type TokenHandlerBox = Box<Box<TokenHandler>>;

pub struct QwenAsrEngine {
    ctx: *mut QwenCtx,
    /// 持有 token 回调的所有权；C 端拿到的是 `&**handler` 派生出来的 raw ptr，
    /// 只要这个 Box 还活着，那个 raw ptr 就有效。Mutex 防止并发 set。
    token_handler: Mutex<Option<TokenHandlerBox>>,
    /// qwen_ctx_t 不是可重入对象；live/fallback/batch 必须串行进入 C 端。
    transcribe_lock: Mutex<()>,
}

/// SAFETY: `qwen_ctx_t` 内部的 pthread/buffer 仅在单次 transcribe 期间被 C 端
/// 自己用；外层不会从两个 Rust 线程并发调进同一个 ctx（由 coordinator 串行
/// 化保证）。Send/Sync 在这一约束下成立。
unsafe impl Send for QwenAsrEngine {}
unsafe impl Sync for QwenAsrEngine {}

#[allow(dead_code)]
pub struct QwenLiveAudioSource {
    live: *mut QwenLiveAudio,
}

/// SAFETY: qwen_live_audio_t protects its mutable buffer with a pthread mutex.
/// Callers must still keep the wrapper alive until any transcribe worker using
/// it has returned.
unsafe impl Send for QwenLiveAudioSource {}
unsafe impl Sync for QwenLiveAudioSource {}

#[allow(dead_code)]
impl QwenLiveAudioSource {
    pub fn create() -> Result<Self> {
        let live = unsafe { qwen_live_audio_create() };
        if live.is_null() {
            anyhow::bail!("qwen_live_audio_create 返回 NULL");
        }
        Ok(Self { live })
    }

    pub fn append_s16le(&self, pcm: &[u8]) -> Result<()> {
        if self.live.is_null() {
            anyhow::bail!("live audio source already freed");
        }
        let rc = unsafe { qwen_live_audio_append_s16le(self.live, pcm.as_ptr(), pcm.len()) };
        if rc != 0 {
            anyhow::bail!("qwen_live_audio_append_s16le failed");
        }
        Ok(())
    }

    pub fn append_f32(&self, samples: &[f32]) -> Result<()> {
        if self.live.is_null() {
            anyhow::bail!("live audio source already freed");
        }
        if samples.len() > i32::MAX as usize {
            anyhow::bail!("too many samples for qwen_live_audio_append_f32");
        }
        let rc = unsafe {
            qwen_live_audio_append_f32(self.live, samples.as_ptr(), samples.len() as i32)
        };
        if rc != 0 {
            anyhow::bail!("qwen_live_audio_append_f32 failed");
        }
        Ok(())
    }

    pub fn finish(&self) {
        if !self.live.is_null() {
            unsafe { qwen_live_audio_finish(self.live) };
        }
    }

    pub fn cancel(&self) {
        if !self.live.is_null() {
            unsafe { qwen_live_audio_cancel(self.live) };
        }
    }

    fn as_ptr(&self) -> *mut QwenLiveAudio {
        self.live
    }
}

impl Drop for QwenLiveAudioSource {
    fn drop(&mut self) {
        if !self.live.is_null() {
            unsafe {
                qwen_live_audio_cancel(self.live);
                qwen_live_audio_free(self.live);
            }
            self.live = ptr::null_mut();
        }
    }
}

impl QwenAsrEngine {
    /// 从模型目录加载（目录里需含 `config.json` / `model.safetensors*` /
    /// `vocab.json` / `merges.txt`，结构见 qwen-asr `download_model.sh`）。
    pub fn load(model_dir: &Path) -> Result<Self> {
        let dir_str = model_dir
            .to_str()
            .with_context(|| format!("model dir 不是合法 UTF-8: {model_dir:?}"))?;
        let c_dir = CString::new(dir_str).context("model dir 含 NUL 字节")?;

        // SAFETY: `c_dir` 在调用期间存活；返回 NULL 表示加载失败。
        let ctx = unsafe { qwen_load(c_dir.as_ptr()) };
        if ctx.is_null() {
            anyhow::bail!("qwen_load 失败：{model_dir:?}");
        }

        Ok(Self {
            ctx,
            token_handler: Mutex::new(None),
            transcribe_lock: Mutex::new(()),
        })
    }

    /// 批式转写：一次性给完整音频（mono f32 16kHz）。
    pub fn transcribe_audio(&self, samples: &[f32]) -> Result<String> {
        let _transcribe = self.begin_transcribe()?;
        let _token_guard = self.clear_token_handler_scoped();
        if samples.len() > i32::MAX as usize {
            anyhow::bail!("too many samples for qwen_transcribe_audio");
        }
        // SAFETY: samples 在调用期间存活；返回是 C `malloc` 出的字符串。
        let raw =
            unsafe { qwen_transcribe_audio(self.ctx, samples.as_ptr(), samples.len() as i32) };
        if raw.is_null() {
            anyhow::bail!("qwen_transcribe_audio 返回 NULL");
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { libc::free(raw as *mut c_void) };
        Ok(text)
    }

    /// 流式转写：内部按 2s chunk 切片，token 通过当前调用的 handler 实时吐出；
    /// 返回值是最终完整文本。
    pub fn transcribe_stream_with_handler<F>(&self, samples: &[f32], handler: F) -> Result<String>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let _transcribe = self.begin_transcribe()?;
        let _token_guard = self.set_token_handler_scoped(handler);
        self.call_transcribe_stream(samples)
    }

    /// no-token final path：清空 token callback 后调用 qwen_transcribe_stream。
    /// C 端在无 callback 且非 live 时会直接走 final refinement，不做 token pseudo-stream。
    #[allow(dead_code)]
    pub fn transcribe_stream_final(&self, samples: &[f32]) -> Result<String> {
        let _transcribe = self.begin_transcribe()?;
        let _token_guard = self.clear_token_handler_scoped();
        self.call_transcribe_stream(samples)
    }

    /// Live source 转写；不注册 token callback，主要用于 fallback/final plumbing。
    /// 需要 token 时使用 transcribe_stream_live_with_handler。
    #[allow(dead_code)]
    pub fn transcribe_stream_live(&self, source: &QwenLiveAudioSource) -> Result<String> {
        let _transcribe = self.begin_transcribe()?;
        let _token_guard = self.clear_token_handler_scoped();
        self.call_transcribe_stream_live(source)
    }

    #[allow(dead_code)]
    pub fn transcribe_stream_live_with_handler<F>(
        &self,
        source: &QwenLiveAudioSource,
        handler: F,
    ) -> Result<String>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let _transcribe = self.begin_transcribe()?;
        let _token_guard = self.set_token_handler_scoped(handler);
        self.call_transcribe_stream_live(source)
    }

    fn call_transcribe_stream(&self, samples: &[f32]) -> Result<String> {
        if self.ctx.is_null() {
            anyhow::bail!("engine already freed — cannot transcribe");
        }
        if samples.len() > i32::MAX as usize {
            anyhow::bail!("too many samples for qwen_transcribe_stream");
        }
        let raw =
            unsafe { qwen_transcribe_stream(self.ctx, samples.as_ptr(), samples.len() as i32) };
        if raw.is_null() {
            anyhow::bail!("qwen_transcribe_stream 返回 NULL");
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { libc::free(raw as *mut c_void) };
        Ok(text)
    }

    fn call_transcribe_stream_live(&self, source: &QwenLiveAudioSource) -> Result<String> {
        if self.ctx.is_null() {
            anyhow::bail!("engine already freed — cannot transcribe");
        }
        if source.as_ptr().is_null() {
            anyhow::bail!("live audio source already freed");
        }
        let raw = unsafe { qwen_transcribe_stream_live(self.ctx, source.as_ptr()) };
        if raw.is_null() {
            anyhow::bail!("qwen_transcribe_stream_live 返回 NULL");
        }
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { libc::free(raw as *mut c_void) };
        Ok(text)
    }

    fn begin_transcribe(&self) -> Result<QwenTranscribeGuard<'_>> {
        if self.ctx.is_null() {
            anyhow::bail!("engine already freed — cannot transcribe");
        }
        let guard = self
            .transcribe_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(QwenTranscribeGuard { _guard: guard })
    }

    fn set_token_handler_scoped<F>(&self, handler: F) -> QwenTokenHandlerGuard<'_>
    where
        F: FnMut(&str) + Send + 'static,
    {
        self.clear_token_handler();
        let boxed: TokenHandlerBox = Box::new(Box::new(handler));
        // boxed 的内部 `Box<TokenHandler>` 在堆上有稳定地址；取它的 &mut 转 raw。
        let userdata = boxed.as_ref() as *const Box<TokenHandler> as *mut c_void;
        {
            let mut slot = self
                .token_handler
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !self.ctx.is_null() {
                unsafe {
                    qwen_set_token_callback(self.ctx, Some(token_trampoline), userdata);
                }
            }
            *slot = Some(boxed);
        }
        QwenTokenHandlerGuard { engine: self }
    }

    fn clear_token_handler_scoped(&self) -> QwenTokenHandlerGuard<'_> {
        self.clear_token_handler();
        QwenTokenHandlerGuard { engine: self }
    }

    fn clear_token_handler(&self) {
        let mut slot = self
            .token_handler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // 先把 C 端那一侧切干净，再 drop 旧 Box，避免 C 在替换瞬间还持有旧指针。
        if !self.ctx.is_null() {
            unsafe { qwen_set_token_callback(self.ctx, None, ptr::null_mut()) };
        }
        *slot = None;
    }
}

struct QwenTranscribeGuard<'a> {
    _guard: MutexGuard<'a, ()>,
}

struct QwenTokenHandlerGuard<'a> {
    engine: &'a QwenAsrEngine,
}

impl Drop for QwenTokenHandlerGuard<'_> {
    fn drop(&mut self) {
        self.engine.clear_token_handler();
    }
}

impl Drop for QwenAsrEngine {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            // 先解绑回调，避免 C 端在 free 后还持有 userdata 指针。
            unsafe {
                qwen_set_token_callback(self.ctx, None, ptr::null_mut());
                qwen_free(self.ctx);
            }
            self.ctx = ptr::null_mut();
        }
        // token_handler 的 Box 由 Mutex 析构时释放。
    }
}

/// C 蹦床：把 `userdata` 解回 `&mut Box<TokenHandler>` 并转发字符串。
unsafe extern "C" fn token_trampoline(piece: *const c_char, userdata: *mut c_void) {
    if userdata.is_null() || piece.is_null() {
        return;
    }
    // SAFETY: userdata 是 set_token_handler 注册的 `*Box<TokenHandler>`。
    let handler: &mut Box<TokenHandler> = unsafe { &mut *(userdata as *mut Box<TokenHandler>) };
    let text = unsafe { CStr::from_ptr(piece) }.to_string_lossy();
    handler(&text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;
    use std::ptr::NonNull;

    fn test_engine_with_ctx(ctx: *mut QwenCtx) -> QwenAsrEngine {
        QwenAsrEngine {
            ctx,
            token_handler: Mutex::new(None),
            transcribe_lock: Mutex::new(()),
        }
    }

    #[test]
    fn qwen_live_audio_source_appends_finishes_and_drops() {
        let source = QwenLiveAudioSource::create().expect("create live source");
        source
            .append_s16le(&[0x00, 0x00, 0x00, 0x80, 0xff, 0x7f])
            .expect("append s16le");
        source.append_f32(&[0.0, 0.25, -0.5]).expect("append f32");
        assert!(source.append_s16le(&[0x01]).is_err());

        source.finish();
        assert!(source.append_s16le(&[0x00, 0x00]).is_err());
    }

    #[test]
    fn qwen_live_audio_source_cancel_rejects_more_audio() {
        let source = QwenLiveAudioSource::create().expect("create live source");
        source.append_s16le(&[0x00, 0x00]).expect("append s16le");

        source.cancel();
        assert!(source.append_f32(&[0.0]).is_err());
    }

    #[test]
    fn qwen_token_handler_guard_clears_slot() {
        let engine = test_engine_with_ctx(ptr::null_mut());

        {
            let _guard = engine.set_token_handler_scoped(|_piece: &str| {});
            assert!(engine
                .token_handler
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some());
        }

        assert!(engine
            .token_handler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_none());
    }

    #[test]
    fn qwen_transcribe_lock_blocks_parallel_entry() {
        let engine = ManuallyDrop::new(test_engine_with_ctx(
            NonNull::<QwenCtx>::dangling().as_ptr(),
        ));

        let guard = engine.begin_transcribe().expect("begin transcribe");
        assert!(engine.transcribe_lock.try_lock().is_err());
        drop(guard);
        assert!(engine.transcribe_lock.try_lock().is_ok());
    }
}
