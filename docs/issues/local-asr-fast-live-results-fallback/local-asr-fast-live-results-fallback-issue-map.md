# 本地 ASR 快速出结果与兜底机制 GitHub Issue Map

Status: published
Date: 2026-07-06
Target repository: seabyte7/openless
Source requirements: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-requirements.md
Source technical spec: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-technical-spec.md
Source development plan: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-development-plan.md
Source validation plan: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-validation-plan.md

## Publishing Status

Remote issue creation completed on `seabyte7/openless`.

- `seabyte7/openless` has GitHub Issues enabled. Current check: `hasIssuesEnabled=true`.
- The upstream technical artifacts are confirmed:
  - Requirements: `confirmed`
  - Technical spec: `confirmed`
  - Development plan: `confirmed`
  - Validation plan: `confirmed`
- The final issue titles, boundaries, and body drafts were confirmed by the user before remote mutation.

Upstream repository note: `Open-Less/openless` has Issues enabled, but current permission is `READ`, so it is not a valid publishing target for this workflow.

## Split Strategy

Use one tracking parent issue plus ten focused child issues. The split is intentionally finer than the five implementation phases so each issue has a clear owner boundary, acceptance criteria, and validation surface.

Production behavior should only be enabled after the safety issues are complete. In particular, live ASR plumbing can be implemented and tested before normal dictation is wired to use it.

## Published Issue Graph

| ID | GitHub issue | Type | Title | Depends on | Primary docs |
| --- | --- | --- | --- | --- | --- |
| P0 | https://github.com/seabyte7/openless/issues/1 | Parent | 本地 Qwen3-ASR 快速出结果与可靠兜底总线 | None | requirements, technical spec, development plan, validation plan |
| C1 | https://github.com/seabyte7/openless/issues/2 | Child | 补齐本地 ASR 基线与 stop-to-visible 计时日志 | P0 | Phase 1, spec section 5 |
| C2 | https://github.com/seabyte7/openless/issues/3 | Child | 为 vendored Qwen runtime 暴露 app-owned live audio source | P0 | Phase 2, spec section 2 |
| C3 | https://github.com/seabyte7/openless/issues/4 | Child | Rust Qwen FFI/engine wrapper 支持 live source、final path 和串行保护 | C2 | Phase 2, spec section 3 |
| C4 | https://github.com/seabyte7/openless/issues/5 | Child | LocalQwenAsr 实现 live-first 状态机和非阻塞 feeder | C3 | Phase 3, spec section 4 |
| C5 | https://github.com/seabyte7/openless/issues/6 | Child | 实现完整 PCM fallback、hard-stall fresh-engine 兜底和失败分类 | C4 | Phase 3, spec section 4 |
| C6 | https://github.com/seabyte7/openless/issues/7 | Child | 将 normal dictation 接入 live mode 并保持 QA/history/Less Computer batch-only | C5 | Phase 4, spec sections 1 and 4 |
| C7 | https://github.com/seabyte7/openless/issues/8 | Child | 增加 session-aware token/capsule payload 并过滤旧 session 输出 | C6 | Phase 4, spec section 6 |
| C8 | https://github.com/seabyte7/openless/issues/9 | Child | 加固 LocalAsrCache busy/deferred release，避免释放活跃 live worker | C3, C5 | Phase 4, spec section 7 |
| C9 | https://github.com/seabyte7/openless/issues/10 | Child | 增加 forced fallback/debug hooks 与自动化测试覆盖 | C4, C5, C7, C8 | Phase 5, validation plan |
| C10 | https://github.com/seabyte7/openless/issues/11 | Child | 完成真机性能验证、兼容性验证和发布证据 | C1, C6, C7, C8, C9 | Phase 5, validation plan |

## Published Issue URLs

- P0: https://github.com/seabyte7/openless/issues/1
- C1: https://github.com/seabyte7/openless/issues/2
- C2: https://github.com/seabyte7/openless/issues/3
- C3: https://github.com/seabyte7/openless/issues/4
- C4: https://github.com/seabyte7/openless/issues/5
- C5: https://github.com/seabyte7/openless/issues/6
- C6: https://github.com/seabyte7/openless/issues/7
- C7: https://github.com/seabyte7/openless/issues/8
- C8: https://github.com/seabyte7/openless/issues/9
- C9: https://github.com/seabyte7/openless/issues/10
- C10: https://github.com/seabyte7/openless/issues/11

## Parent Issue Body Draft

Title: 本地 Qwen3-ASR 快速出结果与可靠兜底总线

Body:

```markdown
## 背景

本地 `local-qwen3` 普通听写现在主要在用户停止录音后才进行整段 Qwen ASR，较长录音会让用户在 stop 之后继续等待数秒到十几秒。目标是让普通听写在录音期间开始处理音频，停止后尽快拿到 raw ASR，同时保留完整 PCM 兜底，避免快速路径失败时影响可用结果。

## 范围

- 仅覆盖 macOS `local-qwen3` 普通听写。
- 第一版不改变 QA voice、Less Computer voice、history retranscription、非本地 ASR provider、Windows Foundry Local Whisper、Windows sherpa-onnx。
- 不新增用户可见设置项，fallback 成功默认只写日志。

## 子 issue

- [ ] C1 补齐本地 ASR 基线与 stop-to-visible 计时日志
- [ ] C2 为 vendored Qwen runtime 暴露 app-owned live audio source
- [ ] C3 Rust Qwen FFI/engine wrapper 支持 live source、final path 和串行保护
- [ ] C4 LocalQwenAsr 实现 live-first 状态机和非阻塞 feeder
- [ ] C5 实现完整 PCM fallback、hard-stall fresh-engine 兜底和失败分类
- [ ] C6 将 normal dictation 接入 live mode 并保持 QA/history/Less Computer batch-only
- [ ] C7 增加 session-aware token/capsule payload 并过滤旧 session 输出
- [ ] C8 加固 LocalAsrCache busy/deferred release，避免释放活跃 live worker
- [ ] C9 增加 forced fallback/debug hooks 与自动化测试覆盖
- [ ] C10 完成真机性能验证、兼容性验证和发布证据

## 验收总目标

- 短录音 `<=2s`: `stop-to-final raw ASR latency <= 1.5s`，或 direct/final path 实测更快且可靠。
- 中等录音 `5-15s`: `stop-to-final raw ASR latency <= 3s`。
- 长录音 `30-60s`: `stop-to-final raw ASR latency <= 5s`，或相对当前基线降低至少 60%。
- live path 启动失败、超时、hard-stall、空/无效结果时自动使用完整 PCM buffer fallback。
- fallback 成功时用户正常获得 transcript；live 和 fallback 都失败时用户看到清晰本地 ASR 错误。
- 日志必须拆分 raw ASR、LLM polish、insert、stop-to-visible。
- cancel、快速重录、engine release 不能造成旧 token、旧 transcript 或 worker 泄漏。

## 规范文档

- Requirements: `docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-requirements.md`
- Technical spec: `docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-technical-spec.md`
- Development plan: `docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-development-plan.md`
- Validation plan: `docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-validation-plan.md`

## 关闭策略

每个子 issue 由对应 PR 使用 `Closes #<child-issue-number>` 关闭。父 issue 在所有子 issue 完成并通过 C10 发布证据后关闭。
```

## Child Issue Body Drafts

### C1 - 补齐本地 ASR 基线与 stop-to-visible 计时日志

```markdown
## 目标

在改变 ASR 行为前，补齐可对比的本地 ASR、LLM polish、insert、stop-to-visible 计时日志，支持后续用同一批短/中/长录音做 before/after 对比。

## 范围

- 记录 `recording_stop_at`、raw ASR 完成、LLM 完成、insert 完成。
- 为 `local-qwen3` 记录 `audio_ms`、model id、cache loaded/reuse、current path。
- 统一日志前缀 `[local-asr fast]` 和 `[coord timing]`。
- 保留现有 `asr_elapsed_ms` / `llm_elapsed_ms` capsule 行为。

## 不在范围

- 不启用 live ASR。
- 不改变 provider 选择、转写结果、润色、插入、历史记录。

## 验收

- 当前 batch 路径下可以从日志拆分 raw ASR、LLM polish、insert 和 stop-to-visible。
- 日志包含模型加载或缓存命中、录音时长、ASR 路径和最终来源。
- 可以用同一批 `<=2s`、`5-15s`、`30-60s` 样本记录 baseline。

## 验证

- 运行新增或调整的 Rust timing helper 单测。
- 运行 `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml local_qwen_transcribe_timeout`。
- 运行 `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml streaming_insert_eligible`。
- 手动记录一组 baseline 日志，作为 C10 对比证据输入。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C2 - 为 vendored Qwen runtime 暴露 app-owned live audio source

```markdown
## 目标

在 vendored Qwen C runtime 中新增 app 可直接 append/finalize/cancel 的 live audio source API，避免复用 stdin-only live reader。

## 范围

- 在 `qwen_asr.h` / `qwen_asr_audio.h` / `qwen_asr_audio.c` 增加:
  - `qwen_live_audio_create`
  - `qwen_live_audio_append_s16le`
  - `qwen_live_audio_append_f32`
  - `qwen_live_audio_finish`
  - `qwen_live_audio_cancel`
  - `qwen_live_audio_free`
- 显式记录 app-owned / stdin-owned、reader thread started、cancelled 等状态。
- `append_s16le` 必须按 little-endian 字节组装 i16，避免未对齐 `int16_t*` 访问。
- live wait loop 和 chunk 边界检查 cancellation，cancel 后尽快退出。
- `free()` 同时正确处理 stdin-created 和 app-created live source。

## 不在范围

- 不接入 Rust provider。
- 不改变普通听写行为。

## 验收

- app-created live source 可以 create、append、finish、cancel、free。
- app-created source free 时不会 join 未启动的 stdin reader thread。
- cancel 能唤醒等待中的 live loop。
- 现有 stdin live reader 行为保持兼容。

## 验证

- 编译 macOS backend，确认 C symbols 正确链接。
- 增加 C/FFI 层最小测试或 debug-only harness，覆盖 append/finish/cancel/free。
- 静态检查 `append_s16le` 没有未对齐 `int16_t*` 强转。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C3 - Rust Qwen FFI/engine wrapper 支持 live source、final path 和串行保护

```markdown
## 目标

在 Rust Qwen wrapper 层暴露 safe-ish live source RAII、live transcribe、no-token final path，并保护同一个 `qwen_ctx_t` 不被并发转写。

## 依赖

- C2 已完成并提供 C live source API。

## 范围

- 在 `qwen_ffi.rs` 增加 `QwenLiveAudio` opaque type 和 live FFI 声明。
- 在 `qwen_engine.rs` 增加 `QwenLiveAudioSource` RAII wrapper，Drop 时 cancel/free。
- 增加 `QwenAsrEngine::transcribe_stream_live(source)`。
- 增加 `QwenAsrEngine::transcribe_stream_final(samples)`，先清空 token callback，再走 no-token final path。
- 给 `QwenAsrEngine` 增加 `transcribe_lock: Mutex<()>`。
- token handler 每次 transcribe 前显式设置，结束后通过 guard 清理。
- `LocalQwenAsr` 需要可获得 `model_id` 和 `model_dir`，为后续 hard-stall fresh-engine fallback 做准备。

## 不在范围

- 不把 live mode 接入普通听写。
- 不实现 provider 状态机和 fallback 策略。

## 验收

- Rust 可以安全创建 live source、append PCM、finish/cancel/free。
- 同一个 engine 的 live/fallback/batch 不会并发进入 C ctx。
- no-token final path 不再走带 token callback 的 pseudo-stream 慢路径。

## 验证

- 新增或调整 Rust 单测覆盖 token handler guard、transcribe lock、live source Drop 行为。
- 编译并链接新增 FFI symbols。
- 静态检查 fallback final path 会清理 token callback。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C4 - LocalQwenAsr 实现 live-first 状态机和非阻塞 feeder

```markdown
## 目标

把 `LocalQwenAsr` 从单纯 buffer provider 改为可测试的 live-first state machine，但在生产普通听写接入前保持范围可控。

## 依赖

- C3 已完成 Rust live source 和 engine wrapper。

## 范围

- 新增 `LocalQwenSessionMode::{DictationLive, BatchOnly}`。
- `consume_pcm_chunk()` 继续保留完整 PCM fallback buffer。
- 使用 bounded non-blocking feeder channel 将 chunk 交给 live source append，recorder callback 不等待 C decoder。
- feeder 满时标记 live path unhealthy，后续交给 fallback policy。
- 录音达到 `LIVE_START_THRESHOLD_MS = 2000` 且 session 仍 recording 时启动 live worker。
- `<2s` 短录音直接走 no-token final path。
- 停止录音时 finish live source，等待 live final 或产生明确 fallback-needed 状态。
- 用 fake engine/live worker 抽取纯 Rust state machine tests。

## 不在范围

- 不实现 hard-stall fresh-engine fallback 完整策略，留给 C5。
- 不把 normal dictation 生产路径切到 live mode，留给 C6。
- 不改前端 token payload，留给 C7。

## 验收

- state machine 覆盖 `Idle -> Buffering -> LiveStarting -> LiveRunning -> Finalizing -> LiveFinal`。
- 短录音 `<2s` 可直接走 final。
- medium/long 录音能启动 live worker 并持续接收 feeder chunk。
- recorder callback 不阻塞，不做大规模 f32 转换，不调用 decoder。
- feeder overflow 不丢完整 PCM buffer。

## 验证

- 单测覆盖 short direct、medium live、feeder overflow、live start fail、cancel before live、cancel while live。
- 静态检查 `consume_pcm_chunk()` 只做轻量 buffer/send。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C5 - 实现完整 PCM fallback、hard-stall fresh-engine 兜底和失败分类

```markdown
## 目标

完成 live path 失败、不适合、超时、hard-stall 时的自动 fallback，保证快速路径不会降低普通听写可靠性。

## 依赖

- C4 已完成 live-first state machine 和 feeder。

## 范围

- fallback 使用完整 PCM buffer，不能依赖 live buffer 是否完整。
- fallback 调用 `transcribe_stream_final()`，不注册 token callback。
- 分类并记录 fallback reason:
  - live source create/start failure
  - feeder overflow / live unhealthy
  - worker panic or join error
  - C 返回 NULL
  - empty/invalid result
  - finalize timeout
  - no token/progress stall
  - live cancel grace exceeded
- live 失败后先 cancel/finish live source，并等待 worker 退出。
- `LIVE_CANCEL_JOIN_GRACE_MS` 建议 1000ms。
- 如果 cached engine 在 cancel grace 内仍 busy，使用 `model_dir` 创建一次性 fresh `QwenAsrEngine` 执行 fallback final。
- fresh fallback engine 完成后立即 drop，不进入 `LocalAsrCache`。
- both-path failure 走清晰本地 ASR 错误路径。

## 不在范围

- 不新增用户可见 fallback 设置。
- 不改变 LLM polish 和 insert 语义。

## 验收

- live 启动失败、超时、空结果、invalid result 都自动 fallback。
- hard-stall 不会让 fallback 无限等待同一个 cached engine。
- fallback 成功时最终结果来源记录为 `fallback`，用户正常获得 transcript。
- live 和 fallback 都失败时用户看到清晰本地 ASR 失败提示。

## 验证

- 单测覆盖 live start fail、timeout、hard-stall fresh engine、invalid result、both-path failure。
- 静态检查 fallback final 清理 token callback。
- 手动或 debug hook 验证 hard-stall 时日志含 `fallback_engine=fresh`。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C6 - 将 normal dictation 接入 live mode 并保持 QA/history/Less Computer batch-only

```markdown
## 目标

把 live-enabled `LocalQwenAsr` 安全接入 macOS `local-qwen3` 普通听写，同时明确排除第一版不覆盖的流程。

## 依赖

- C5 已完成可靠 fallback。

## 范围

- 新增或调整 `build_local_qwen3_for_dictation(inner, session_id)`。
- normal dictation 使用 `LocalQwenSessionMode::DictationLive { session_id }`。
- QA voice、Less Computer voice、history retranscription 使用 `BatchOnly`。
- `resources.cancel()` 接入 live source cancel、feeder shutdown、worker join/fallback buffer 清理。
- `end_session()` 停 recorder 后调用 live-aware `LocalQwenAsr::transcribe()`。
- 现有 correction、polish、translation、streaming insert、history、debug recording 语义不改变。

## 不在范围

- 不改非本地 ASR provider。
- 不改 Windows 本地 ASR。
- 不新增用户可见设置项。

## 验收

- macOS `local-qwen3` 普通听写使用 live mode。
- QA voice、Less Computer voice、history retranscription 不意外使用 live mode。
- cancel、stop、fallback 后不会留下活跃 worker。
- `RawTranscript.duration_ms` 仍表示原始录音时长，不包含 padding 或 fallback internals。

## 验证

- 单测或静态路径检查 normal dictation vs BatchOnly 构造分支。
- 运行 existing coordinator tests。
- 手动 smoke: normal dictation、QA voice、history retranscription。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C7 - 增加 session-aware token/capsule payload 并过滤旧 session 输出

```markdown
## 目标

live path 下 token 更早、更频繁，必须让 token 和 capsule payload 带 session id，避免取消或快速重录时旧 session 输出污染新 session。

## 依赖

- C6 已完成 normal dictation live wiring。

## 范围

- `CapsulePayload` 增加 optional `sessionId`。
- visible/processing capsule 状态携带当前 session id。
- 新增或替换 `LocalAsrTokenPayload`:
  - `sessionId`
  - `provider`
  - `source`
  - `sequence`
  - `piece`
- 后端只为当前 session emit token。
- 前端如果消费 token，必须按 active capsule session id 过滤。
- 如果保留 legacy string event 兼容，前端不能同时消费新旧事件导致重复显示。

## 不在范围

- 不新增新的 UI 设置。
- 不改变最终 inserted text 语义。

## 验收

- 取消的 session 不会插入或保存过期文本。
- 快速重录时旧 session token 不会显示到新 capsule。
- token payload 序列化和前端 filtering helper 有测试覆盖。

## 验证

- 单测覆盖 token payload serialization、capsule session id、frontend session filter。
- 静态搜索 `local-asr-token|LocalAsrTokenPayload|sessionId`，确认没有重复消费。
- 手动验证 cancel 和 rapid re-record。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C8 - 加固 LocalAsrCache busy/deferred release，避免释放活跃 live worker

```markdown
## 目标

确保 keep-loaded cache release 不会在 live worker 或 fallback 仍使用 engine 时造成状态漂移、误释放或后续 ASR 失败。

## 依赖

- C3 提供 transcribe lock 和 engine wrapper。
- C5 提供 fallback/hard-stall busy engine 行为。

## 范围

- `LocalAsrCache::release_now()` / `release_if_idle()` 检查 active strong refs 或新增 busy counter。
- engine busy 时 release 请求记录 `deferred_busy` 并保留 engine。
- `schedule_local_asr_release()` 在 live/fallback worker 全部结束后调用。
- fresh fallback engine 不进入 cache。
- cache 状态日志与实际 busy/released 状态一致。

## 不在范围

- 不改变用户模型选择。
- 不改变 keep-loaded 语义，除非 active worker 要求 defer release。

## 验收

- release while recording/live worker active 不会释放或报告已释放。
- live/fallback 完成后 release 可正常执行。
- hard-stall cached engine 保持 busy，fresh fallback engine 不污染 cache。

## 验证

- 单测 cache busy release guard。
- 手动测试 recording/live active 时触发 release。
- 静态检查 fresh engine 没有写入 `LocalAsrCache`。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C9 - 增加 forced fallback/debug hooks 与自动化测试覆盖

```markdown
## 目标

为 live-first/fallback 机制提供可重复验证的 failure injection 和自动化测试，避免只能靠真实模型手动复现。

## 依赖

- C4, C5, C7, C8 已完成核心状态机、fallback、session token、cache safety。

## 范围

- 增加 debug-only forced fallback knobs 或 test hooks:
  - disable live
  - live source create/start failure
  - live worker timeout after stop
  - live worker hard-stall after cancel
  - live returns empty/invalid final
  - feeder overflow
- 增加 Rust state machine tests:
  - short direct
  - medium live
  - feeder overflow
  - start fail fallback
  - timeout fallback
  - hard-stall fresh-engine fallback
  - invalid result fallback
  - cancel
  - rapid re-record
  - full PCM buffer duration preservation
  - cache release defer
- 增加 token/capsule payload tests。
- 增加 C/FFI checks where feasible。

## 不在范围

- 不要求 CI 运行大模型真实推理。
- 不把 debug hooks 暴露成用户设置。

## 验收

- forced fallback 场景可以稳定触发并验证日志和结果来源。
- 自动化测试覆盖核心状态转换和安全边界。
- 测试不会依赖本地大模型文件。

## 验证

- 运行新增 targeted tests。
- 运行 `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml local_qwen_transcribe_timeout`。
- 运行 `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml streaming_insert_eligible`。
- 运行 `cargo test --manifest-path openless-all/app/src-tauri/backend-tests/Cargo.toml`。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。
```

### C10 - 完成真机性能验证、兼容性验证和发布证据

```markdown
## 目标

用目标 macOS 机器和本地 `qwen3-asr-0.6b` 模型证明速度、兜底、质量和兼容性满足 requirements。

## 依赖

- C1 baseline/timing 已完成。
- C6 normal dictation live wiring 已完成。
- C7 session-aware token 已完成。
- C8 cache release safety 已完成。
- C9 forced fallback/testing 已完成。

## 范围

- 用同一机器、同一模型、同一 provider、同一批样本采集 baseline 与新路径日志。
- 样本:
  - short: `<=2s`
  - medium: `5-15s`
  - long: `30-60s`
- 验证 forced fallback:
  - live start fail
  - worker timeout
  - hard-stall
  - invalid final
  - feeder overflow
- 验证 lifecycle:
  - cancel before live
  - cancel during live
  - cancel while finalizing
  - rapid re-record
  - release while recording/live active
- 验证 compatibility:
  - non-local ASR provider
  - Apple Speech
  - QA voice
  - history retranscription
  - existing polish modes
  - translation mode
  - debug audio recording
  - streaming insert setting behavior

## 验收

- short `stop_to_final_raw_asr_ms <= 1.5s`，或 direct/final path 实测更快且可靠。
- medium `stop_to_final_raw_asr_ms <= 3s`。
- long `stop_to_final_raw_asr_ms <= 5s`，或相对 baseline 降低至少 60%。
- fallback 成功时用户正常得到 transcript。
- both-path failure 有清晰本地 ASR 错误。
- cancel/rapid re-record 无旧 token 或旧 transcript 泄漏。
- engine release active worker 时 defer 或 no-op。
- 非本地和 out-of-scope local flows 不回归。

## 验证

- 运行 `cd openless-all/app && npm run build`。
- 运行 targeted Cargo tests 和 backend-tests。
- 可选运行 `openless-all/app/scripts/build-mac.sh INSTALL=0`。
- 附上 before/after 日志摘要、SLO 表格、fallback 证据、兼容性 checklist。

## 关闭策略

实现 PR 使用 `Closes #<this-issue-number>`。所有子 issue 完成并通过本 issue 发布证据后，关闭父 issue。
```

## Audit Pass 1 - Requirements Coverage

| Requirement area | Covered by issue(s) | Result |
| --- | --- | --- |
| macOS `local-qwen3` 普通听写 live-first | C4, C6 | Covered |
| 停止后快速拿到 raw ASR | C1, C4, C10 | Covered |
| 短录音可 direct/final | C4, C10 | Covered |
| 中长录音边录边处理 | C4, C6, C10 | Covered |
| 完整 PCM fallback | C5 | Covered |
| live start fail / timeout / invalid / hard-stall fallback | C5, C9, C10 | Covered |
| fallback log-only unless both paths fail | C5, C10 | Covered |
| raw ASR / LLM / insert / stop-to-visible 拆分日志 | C1, C10 | Covered |
| cancel 和快速重录 session 安全 | C6, C7, C9, C10 | Covered |
| stale token 过滤 | C7 | Covered |
| engine release 不释放 active worker | C8 | Covered |
| recorder callback 不阻塞 | C4 | Covered |
| hard-stall fresh engine fallback | C5, C8, C9 | Covered |
| 模型选择、keep-loaded、历史、调试录音、插入兼容 | C6, C8, C10 | Covered |
| 非本地 provider 不变 | C6, C10 | Covered |
| QA voice、Less Computer voice、history retranscription out of first live scope | C6, C10 | Covered |
| forced fallback 验证 | C9, C10 | Covered |
| performance SLO 验证 | C1, C10 | Covered |

Audit pass 1 result: no uncovered requirement found.

## Audit Pass 2 - Dependency and Conflict Check

| Check | Result |
| --- | --- |
| Issue order supports implementation without enabling unsafe production behavior too early | Pass. C2/C3/C4 can land plumbing before C6 wires normal dictation. |
| Fallback is not split away from live wiring in a way that would make production unreliable | Pass. C6 depends on C5, so production normal dictation live mode waits for fallback. |
| Session token filtering is not required before live plumbing tests | Pass. C7 depends on C6; C6 can avoid frontend token consumption until C7, or C7 can land immediately after C6 before broad validation. |
| Cache release safety has access to engine wrapper and fallback semantics | Pass. C8 depends on C3 and C5. |
| Validation depends on baseline timing and all safety mechanisms | Pass. C10 depends on C1, C6, C7, C8, C9. |
| QA/history/Less Computer exclusion conflicts with shared provider refactor | Pass. C6 explicitly owns BatchOnly gating and C10 validates no regression. |
| Forced debug hooks risk leaking to users | Pass. C9 explicitly limits hooks to debug-only/test-only surfaces and excludes user settings. |
| Parent issue duplicates child acceptance too much | Pass. Parent only tracks global scope and SLOs; child bodies carry implementation boundaries. |
| Existing technical docs status conflicts with remote publishing | Pass. Requirements, technical spec, development plan, and validation plan are confirmed. |
| GitHub target can accept issues | Pass. `seabyte7/openless` Issues were enabled before publishing. |

Audit pass 2 result: the split has no internal dependency conflict, and remote publishing has been completed against the fork repository.

## Publishing Verification

1. Enabled GitHub Issues on `seabyte7/openless`.
2. Confirmed the technical spec, development plan, validation plan, and final issue boundaries.
3. Created P0 first as https://github.com/seabyte7/openless/issues/1.
4. Created C1-C10 as https://github.com/seabyte7/openless/issues/2 through https://github.com/seabyte7/openless/issues/11.
5. Patched P0 with the real child issue list.
6. Patched child bodies with real dependency issue numbers and exact `Closes #...` instructions.
7. Verified remote issue count, titles, body links, dependencies, and closing strategy.
