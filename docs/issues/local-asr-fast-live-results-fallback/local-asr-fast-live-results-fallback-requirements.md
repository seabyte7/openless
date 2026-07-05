# 本地 ASR 快速出结果与兜底机制 Requirements

Status: confirmed
Work type: feature
Artifact: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-requirements.md

## Goal

让 macOS 本地 ASR 的普通听写体验变快：用户开始录音后，本地识别应尽早开始处理音频，而不是等用户停止录音后才开始完整识别。用户停止录音后，应快速拿到 raw ASR 最终转写结果，并尽快完成后续润色和文本插入。

同时必须保留可靠性：如果快速实时识别路径无法启动、卡住、超时或失败，OpenLess 必须自动走可靠的本地转写兜底路径，并在可能的情况下仍返回可用的录音结果。

## Current State

- 当前 macOS 本地 ASR provider 是 `local-qwen3`。
- 当前 Qwen 本地听写路径在录音期间缓冲 16 kHz 单声道 PCM，停止录音后才运行本地 Qwen 转写。
- 当前运行日志显示，较长录音在停止录音后，进入 LLM 润色前仍可能花数秒到十几秒做本地 ASR。
- vendored Qwen ASR 引擎具备 live streaming transcription 能力，但 app 侧 Rust wrapper 当前只暴露整段音频转写入口。
- 本地 ASR 引擎已有 keep-loaded 缓存机制，所以冷启动不是唯一的体感慢来源。

## Expected Outcome

- macOS `local-qwen3` 普通听写应优先在录音过程中启动本地 ASR 处理。
- 用户停止录音后，OpenLess 应 finalize 已经进行中的本地识别，并尽快得到最终转写文本。
- 如果 live local ASR 不可用或失败，OpenLess 应自动兜底到可靠的本地 batch/final 转写路径。
- 除非最终结果受到影响，用户不需要理解本次使用了 live path 还是 fallback path。
- 日志必须清晰拆分：录音时长、模型加载或缓存命中、live ASR 启动、首个音频 chunk 接收、首个 token、stop-to-final raw ASR、stop-to-visible/inserted text、总 ASR、LLM 润色、插入耗时。

## User Scenarios

1. 短句听写
   - 用户录一条很短的命令或句子。
   - OpenLess 应在停止录音后尽快返回文本。
   - 如果短句使用直接 final/batch 路径比 live path 更快，系统可以选择更快且可靠的路径，但结果行为必须正确。

2. 中长文本听写
   - 用户录一段较长的段落。
   - OpenLess 应在录音过程中持续处理音频，避免用户停止录音后再等待完整 ASR 成本。
   - ASR 完成后，最终文本仍应沿用现有润色和插入行为。

3. live ASR 失败
   - live worker 启动失败、意外退出、卡住或返回无效结果。
   - OpenLess 应使用已捕获的录音 buffer 自动重试兜底转写。
   - 如果兜底成功，用户拿到正常转写；如果 live 和 fallback 都失败，用户看到清晰的本地 ASR 错误。

4. 取消录音或快速重录
   - 用户取消当前录音，或上一条刚结束就开始下一条听写。
   - 旧 session 的 token 不能污染新 session。
   - 引擎释放、取消、兜底都不能留下未退出的旧 worker。

## Business Rules

- 主要用户价值是降低停止录音后的等待时间，即 `stop-to-final ASR latency`，不只是降低总 CPU 时间。
- 用户感知结果分两层衡量：主指标是 `stop-to-final raw ASR latency`，次指标是 `stop-to-visible/inserted text latency`。LLM 润色耗时必须单独记录，不能混入 ASR 耗时。
- 快速路径不能让普通听写比当前本地 Qwen 路径更不可靠。
- fallback 必须自动触发，不要求用户手动操作；fallback 默认只写入日志，不显示额外提示，除非 live 和 fallback 都失败，或最终结果明显受到影响。
- 同一模型下，live path 转写质量应至少接近当前本地 Qwen 结果；如果速度和质量冲突，fallback 侧优先可靠性和质量。
- 现有模型选择、keep-loaded 行为、润色模式、历史记录、调试录音和插入行为必须继续兼容。
- 本需求默认只覆盖 macOS `local-qwen3` 普通听写，除非后续确认扩大范围。
- very short audio 可以选择 direct/final path 而不是 live path，前提是实测更快且结果可靠。
- ASR 后台处理不能阻塞录音采集，不能导致音频 chunk 丢失、UI 明显卡顿或停止录音失败。

## In Scope

- macOS `local-qwen3` 普通听写的快速 live 本地 ASR 行为。
- live path 失败、不适合或超时时的可靠 fallback。
- 能区分 ASR、LLM 润色和插入耗时的日志。
- 取消、快速重录、旧 token 污染等 session 安全问题。
- 保持现有最终转写、润色、历史记录和插入语义。

## Out of Scope

- 替换 Qwen3-ASR 为其他模型家族。
- 优化 Volcengine、Bailian、Whisper-compatible API、MiMo 等云端 ASR provider。
- 改动 DeepSeek 或其他 LLM 润色 provider 的行为；本需求只要求记录润色耗时。
- Windows Foundry Local Whisper 和 Windows sherpa-onnx 行为。
- 新增用户可见设置项，除非后续确认确实必要。
- 在本步骤创建 GitHub issue、技术 spec 或实现代码。

## Impacted Surfaces

- 使用 `local-qwen3` 的 macOS 普通听写流程。
- vendored Qwen ASR 引擎的本地 ASR runtime wrapper。
- recorder 到 ASR 的音频交接。
- 胶囊和本地 token 展示行为。
- 听写取消和 session 生命周期。
- 本地 ASR 日志与诊断。
- fallback 转写和错误上报。

## Acceptance Criteria

1. 快速出结果
   - 使用本地 Qwen 普通听写时，在 fast live path 可用的情况下，ASR 应在录音过程中开始工作。
   - 对中长录音，停止录音到拿到最终 raw ASR 文本的耗时应明显低于当前“停止后再整段转写”的行为。
   - 在同一台机器、同一模型、模型已 warm/cache 命中的常规环境下，性能目标为：
     - 2 秒以内短录音：`stop-to-final raw ASR latency <= 1.5s`，或采用实测更快的 direct/final path。
     - 5-15 秒中等录音：`stop-to-final raw ASR latency <= 3s`。
     - 30-60 秒长录音：`stop-to-final raw ASR latency <= 5s`，或相对当前基线降低至少 60%。
   - 如果 LLM 润色开启，raw ASR 完成时间和最终插入时间都必须记录；快速 ASR 验收不能被润色耗时掩盖。

2. 兜底行为
   - 如果 live path 无法启动、意外退出、停止录音后超过兜底超时仍无 final 结果、或返回明显无效结果，OpenLess 自动使用已捕获音频尝试本地 fallback 转写。
   - fallback 必须基于完整录音缓存，不能依赖 live buffer 是否完整，也不能破坏后续调试录音、历史记录和插入流程。
   - 兜底触发应有明确日志，包含触发原因、触发时间、fallback ASR 耗时和最终来源。
   - 如果 fallback 成功，用户收到正常 transcript，后续润色和插入继续执行。
   - 如果 live 和 fallback 都失败，用户收到清晰的本地 ASR 失败提示。

3. session 安全
   - 已取消的 session 不能插入或保存过期转写文本。
   - 上一个 session 不能向新 session 发出 token。
   - 引擎释放不能发生在仍有 live transcription worker 持有引擎时。
   - stop、cancel、fallback、快速重录之后不能留下未退出的 live worker，也不能让旧 worker 继续占用引擎或污染日志归因。

4. 可观测性
   - 日志包含模型加载或缓存命中、live ASR 启动、首个音频接收、首个 token、stop-to-final raw ASR、stop-to-visible/inserted text、总 ASR、润色和插入耗时。
   - 日志能明确标识最终结果来自 `live`、`fallback` 还是 `failure`。
   - 日志必须支持同一批短、中、长录音样本在改动前后做耗时对比。

5. 行为保持
   - 现有润色模式、历史记录、调试录音和插入行为继续兼容本地 Qwen ASR。
   - 非本地 ASR provider 行为不发生变化。
   - QA voice、Less Computer voice 和历史记录重转写不属于第一版交付范围；如果这些流程复用相同底层代码，必须保持不回归。

6. 验证方式
   - 实现后必须用短、中、长三类录音样本验证。
   - 验证必须先记录当前实现的基线，再用同一批样本比较新 fast path 的 `stop-to-final raw ASR latency`、最终插入耗时、转写结果和 fallback 结果。
   - 验证必须覆盖 forced fallback 场景，包括 live 启动失败、live worker 超时、live 返回无效结果、取消录音、快速连续录音。

## Open Questions

- None. 用户已确认采用上述速度指标、兜底策略和第一版范围。

## Confirmation Gate

Requirements confirmed by user on 2026-07-05. Next step: enter `issue-spec-plan` before implementation.
