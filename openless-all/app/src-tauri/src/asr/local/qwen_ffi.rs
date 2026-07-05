//! 对 vendored Open-Less/qwen-asr 公共 C API 的最小 FFI 声明。
//!
//! 头文件见 `vendor/qwen-asr/qwen_asr.h`。这里**不**复刻 `qwen_ctx_t`
//! 内部布局——保持不透明指针即可，避免 pthread/对齐相关的脆弱假设。

use std::os::raw::{c_char, c_int, c_void};

/// 不透明的 qwen_ctx_t；只通过指针来回传。
#[repr(C)]
pub struct QwenCtx {
    _opaque: [u8; 0],
}

/// 不透明的 qwen_live_audio_t；只通过 C API 创建、追加、结束、取消和释放。
#[allow(dead_code)]
#[repr(C)]
pub struct QwenLiveAudio {
    _opaque: [u8; 0],
}

/// `typedef void (*qwen_token_cb)(const char *piece, void *userdata);`
pub type QwenTokenCb = unsafe extern "C" fn(piece: *const c_char, userdata: *mut c_void);

// 用经典 `extern "C"` block 而非 `unsafe extern "C"` block — 后者需 Rust
// 1.82+；CLAUDE.md 声明 rust-version = "1.77"，避免给二次贡献者制造毛刺。
extern "C" {
    pub fn qwen_load(model_dir: *const c_char) -> *mut QwenCtx;
    pub fn qwen_free(ctx: *mut QwenCtx);

    pub fn qwen_set_token_callback(
        ctx: *mut QwenCtx,
        cb: Option<QwenTokenCb>,
        userdata: *mut c_void,
    );
    pub fn qwen_set_prompt(ctx: *mut QwenCtx, prompt: *const c_char) -> c_int;
    pub fn qwen_set_force_language(ctx: *mut QwenCtx, language: *const c_char) -> c_int;
    pub fn qwen_supported_languages_csv() -> *const c_char;

    pub fn qwen_transcribe_audio(
        ctx: *mut QwenCtx,
        samples: *const f32,
        n_samples: c_int,
    ) -> *mut c_char;
    pub fn qwen_transcribe_stream(
        ctx: *mut QwenCtx,
        samples: *const f32,
        n_samples: c_int,
    ) -> *mut c_char;

    pub fn qwen_live_audio_create() -> *mut QwenLiveAudio;
    pub fn qwen_live_audio_append_s16le(
        live: *mut QwenLiveAudio,
        buf: *const u8,
        n_bytes: usize,
    ) -> c_int;
    pub fn qwen_live_audio_append_f32(
        live: *mut QwenLiveAudio,
        samples: *const f32,
        n_samples: c_int,
    ) -> c_int;
    pub fn qwen_live_audio_finish(live: *mut QwenLiveAudio);
    pub fn qwen_live_audio_cancel(live: *mut QwenLiveAudio);
    pub fn qwen_live_audio_free(live: *mut QwenLiveAudio);
    pub fn qwen_transcribe_stream_live(ctx: *mut QwenCtx, live: *mut QwenLiveAudio) -> *mut c_char;
}
