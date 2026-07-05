# 本地 ASR 快速出结果与兜底机制 Development Plan

Status: draft
Requirements: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-requirements.md
Technical spec: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-technical-spec.md
Artifact: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-development-plan.md

## Phase 1: Baseline and Timing Plumbing

- Goal: 在改行为前补齐可对比的 ASR、LLM、insert、stop-to-visible 计时口径。
- Scope: 只加日志、计时结构和最小测试，不改变 ASR 行为。
- Changes:
  - 在 coordinator 记录 `recording_stop_at`、raw ASR 完成、LLM 完成、insert 完成。
  - 为 `local-qwen3` 记录 `audio_ms`、model id、cache loaded/reuse、current path。
  - 统一日志前缀 `[local-asr fast]` 和 `[coord timing]`。
  - 保留当前 `asr_elapsed_ms` / `llm_elapsed_ms` capsule 行为。
- Tests:
  - 新增或调整纯 Rust timing helper 单测。
  - 运行 targeted Cargo 单测，确认现有 timeout 和 streaming insert tests 不变。
- Acceptance:
  - 当前路径下可以从日志区分 raw ASR、LLM polish、insert。
  - 能用同一批短/中/长录音记录 baseline。
- Rollback:
  - 删除 timing helper 和日志调用即可，不影响 ASR 行为。

## Phase 2: C Live Source and Rust FFI Wrapper

- Goal: 给 app 提供可直接 append/finalize/cancel 的 Qwen live audio source。
- Scope: vendored Qwen C runtime、Rust FFI、engine wrapper；不接入普通听写流程。
- Changes:
  - 在 `qwen_asr_audio.h/c` 增加 `qwen_live_audio_create`、`append_s16le`、`append_f32`、`finish`、`cancel`、`free`。
  - 在 `qwen_asr.h` 的 live struct 增加 cancellation、ownership 和 reader-thread-started 状态，或用等效内部状态实现。
  - `append_s16le` 按 little-endian 字节显式组装 i16，避免未对齐 `int16_t*` 访问。
  - 在 `qwen_asr.c::stream_impl` 的 live wait loop 和 chunk 边界检查 cancellation。
  - 在 `qwen_ffi.rs` 增加 `QwenLiveAudio` opaque type 和 live FFI 声明。
  - 在 `qwen_engine.rs` 增加 RAII `QwenLiveAudioSource`、live transcribe 方法、no-token final 方法、transcribe mutex。
- Tests:
  - 编译 macOS backend，确保 C symbols 正确链接。
  - 单测或 debug-only test 验证 live source append/finish/cancel 不 panic、不死锁。
  - 单测验证 app-created live source free 时不会 join 未启动的 stdin reader thread。
  - 单测确认 no-token final path 会清理 token handler。
- Acceptance:
  - Rust 可以创建 live source、append PCM、finish/cancel/free。
  - 同一 engine 不允许 live/fallback/batch 并发进入 C ctx。
- Rollback:
  - 移除新增 FFI 和 C symbols；现有 batch/stream ASR 不受影响。

## Phase 3: LocalQwenAsr Live-First State Machine

- Goal: 在普通 dictation provider 内实现 live-first、short direct final、full PCM fallback。
- Scope: `LocalQwenAsr` 和 session lifecycle；不改 polish/insert 语义。
- Changes:
  - 增加 `LocalQwenSessionMode::{DictationLive, BatchOnly}`。
  - 普通 dictation 创建 `DictationLive { session_id }`；QA/重转写保留 `BatchOnly`。
  - `consume_pcm_chunk()` 只追加 fallback buffer，并通过 bounded non-blocking feeder channel 送 chunk，不直接调用 C decoder。
  - feeder 满时标记 live unhealthy，后续通过完整 PCM buffer fallback；recorder callback 不能阻塞。
  - 录音达到 2s 后启动 live worker；停止前不足 2s 则直接 no-token final。
  - 停止录音后 `finish()` live source 并等待 final。
  - live 失败、超时、空结果、无效结果时 cancel live 并使用完整 PCM buffer fallback。
  - 如果 live cancel 后短时间内无法退出并释放 cached engine，加载一次性 fresh engine 执行 fallback final，完成后立即 drop。
  - fallback 走 `transcribe_stream_final()`，不注册 token callback。
  - `cancel()` 清理 buffer、关闭 feeder、cancel live source，并防止旧结果上屏。
- Tests:
  - 用 fake engine/live worker 抽取纯 Rust state machine tests。
  - 覆盖 short direct、medium live、feeder overflow、live start fail、live timeout、live hard-stall fresh-engine fallback、invalid result fallback、cancel、rapid re-record。
- Acceptance:
  - `LocalQwenAsr` 可以在不依赖真实模型的测试里证明状态转换和 fallback 决策。
  - 真实模型路径仍返回 `RawTranscript` 并保留原始 duration。
- Rollback:
  - normal dictation 构造切回 `BatchOnly`，保留新增 FFI 但不启用。

## Phase 4: Coordinator, Token Payload, Cache Safety

- Goal: 把 live provider 安全接入普通听写，并补齐 session-aware token 和 engine release 保护。
- Scope: coordinator wiring、resources cancel、cache release、frontend token event type。
- Changes:
  - 新增 `build_local_qwen3_for_dictation(inner, session_id)`，只在普通 dictation local-qwen3 分支使用。
  - `build_qa_asr_start()`、history retranscription、Less Computer voice 保持 batch-only。
  - `CapsulePayload` 增加 optional `sessionId`，visible/processing 状态带当前 session。
  - 后端 token event 改为 `{ sessionId, provider, source, sequence, piece }`。
  - 前端如消费 token，必须按当前 capsule `sessionId` 过滤；若不消费 token，保持无 UI 变更。
  - `LocalAsrCache::release_now()` / `release_if_idle()` 对 busy engine defer release，避免 active worker 期间释放状态漂移。
  - `schedule_local_asr_release()` 在 live/fallback worker 全部结束后调用。
- Tests:
  - 单测 cache busy release guard。
  - 单测 capsule payload session id、token payload serialization / session filtering helper。
  - 运行 existing coordinator tests，包括 `local_qwen_transcribe_timeout` 和 `streaming_insert_eligible`。
- Acceptance:
  - 普通 dictation 使用 live mode。
  - QA/历史重转写不会意外使用 live mode。
  - 取消和快速重录不会接收旧 session token。
- Rollback:
  - 将 dictation wiring 切回 batch constructor；token payload 可保持但停止 emit live tokens。

## Phase 5: End-to-End Validation and Tuning

- Goal: 用同一批样本证明速度、兜底、质量和兼容性满足 requirements。
- Scope: 本地 macOS 真机验证、日志审查、必要的阈值微调。
- Changes:
  - 增加 debug-only forced fallback knobs，例如禁用 live、模拟 live timeout、模拟 invalid result。
  - 增加 debug-only hard-stall/fresh-engine fallback 验证入口，确保 stuck live worker 不会阻断 fallback。
  - 记录 baseline 和新路径同样样本的日志。
  - 如果 2s live threshold 对 2-5s 录音不理想，在不改需求的前提下调 threshold。
- Tests:
  - 短录音 `<=2s`、中等录音 `5-15s`、长录音 `30-60s`。
  - forced fallback：live start fail、worker timeout、worker hard-stall、invalid final。
  - cancel、rapid re-record、release while recording。
  - `npm run build` 和 targeted Cargo tests。
- Acceptance:
  - 短录音 stop-to-final raw ASR `<=1.5s` 或 direct final 更快。
  - 中等录音 stop-to-final raw ASR `<=3s`。
  - 长录音 stop-to-final raw ASR `<=5s` 或比 baseline 降低至少 60%。
  - fallback 成功时用户正常得到 transcript；两路失败时显示清晰错误。
  - 工作区不包含与本 issue 无关的修改。
- Rollback:
  - 关闭 live constructor，保留 timing 日志和 batch fallback。

## Delivery Notes

- 这些 phase 是实现顺序，不代表要把用户可见能力拆成多个发布阶段。
- 对外可作为一个 issue-delivery 完成：live-first + fallback + telemetry + validation 一起交付。
- 任一 phase 发现与 requirements 冲突的产品决策，应回到 issue-definition，而不是在实现中静默改需求。
