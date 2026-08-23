/* App 自有 shim：设置 vendored qwen-asr 没有暴露 setter 的流式解码参数。
 *
 * 为什么需要它：vendored 库把 `past_text_conditioning` 默认成 0
 * （`qwen_asr.c` 的 `qwen_load`），但那个默认值只对 segmented 路径成立；
 * CLI 在 `--stream` 下会强制置 1（`main.c` 的 `else if (stream_mode)`）。
 * App 直接调库、从不碰这些字段，于是长期跑在一个官方 CLI 从不使用的配置下：
 * 每个 2s chunk 都在无文本前缀地重解全文，撞上 `stream_impl` 那套 append-only
 * 提交逻辑，产出「中间重复整段」+「中段/尾部吞字」。回归护栏见
 * `scripts/macos-qwen-stream-ab.sh`。
 *
 * 为什么写在 C 这一侧：`src/asr/local/qwen_ffi.rs` 刻意让 `qwen_ctx_t` 保持
 * 不透明指针，以避免在 Rust 里复刻结构体布局（pthread/对齐假设很脆）。这里
 * 真实头文件在作用域内，能直接改字段而不破坏那个约束。
 *
 * 为什么不放进 vendor/qwen-asr/：app 自有可以绕开
 * `docs/qwen-asr-submodule-upgrade-checklist.md` 那套 fork 提交 + 指针 bump +
 * CI 重跑的流程。副作用是若将来 submodule bump 改了字段名，本文件会编译失败
 * —— 这正是期望的、响亮的失败方式，比静默失效好。
 *
 * 只设 `past_text_conditioning` 一个字段：实测已确认 `stream_max_new_tokens=32`
 * 与其余默认值都是对的（打开 conditioning 后 max_new 从未被触顶），做成通用
 * params 结构体只会引入并不需要的漂移面。
 */

#include "qwen_asr.h"

/* 打开流式 past-text conditioning。返回 0 成功，-1 失败。
 * 签名沿用库内既有 setter 约定（`qwen_set_prompt` / `qwen_set_force_language`）。*/
int openless_qwen_enable_stream_past_text(qwen_ctx_t *ctx) {
    if (!ctx) {
        return -1;
    }
    ctx->past_text_conditioning = 1;
    return 0;
}
