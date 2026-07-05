# 本地 ASR 快速出结果与兜底机制 Validation Plan

Status: confirmed
Requirements: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-requirements.md
Technical spec: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-technical-spec.md
Development plan: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-development-plan.md
Artifact: docs/issues/local-asr-fast-live-results-fallback/local-asr-fast-live-results-fallback-validation-plan.md

## Project Testing Profile

No `.codex/testing-profile.md` or `docs/ai/testing-profile.md` exists in this repository.

Use repo-native checks:

- Frontend build: `cd openless-all/app && npm run build`
- Backend targeted tests: `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml local_qwen_transcribe_timeout`
- Coordinator regression tests: `cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml streaming_insert_eligible`
- Backend pure harness where relevant: `cargo test --manifest-path openless-all/app/src-tauri/backend-tests/Cargo.toml`
- macOS app smoke/build when implementation is complete: `openless-all/app/scripts/build-mac.sh INSTALL=0`

Large Qwen model inference is machine/model dependent and should be validated manually on the target macOS machine with the downloaded `qwen3-asr-0.6b` model.

## Automated Checks

### Rust Unit Tests

Add tests for:

- `live_finalize_timeout(audio_secs)` returns requirements-compatible budgets.
- short audio `< 2s` routes to direct final path.
- `>= 2s` normal dictation starts live worker.
- recorder callback uses a bounded non-blocking feeder and routes overflow to fallback without blocking capture.
- live start failure routes to fallback.
- live timeout routes to fallback after cancelling live source.
- live hard-stall after cancel grace uses one-shot fresh fallback engine or records both-path failure if fresh engine cannot load.
- invalid/empty live result routes to fallback.
- user cancellation does not fallback or insert text.
- rapid re-record ignores old session token/result.
- fallback uses full PCM buffer and preserves original `duration_ms`.
- cache release defers while engine is busy.
- token payload contains `sessionId`, `provider`, `source`, `sequence`, and `piece`.
- capsule payload exposes optional `sessionId`, and frontend token filtering uses that active session id.

### C / FFI Checks

Add or verify tests/checks for:

- `qwen_live_audio_create()` initializes mutex/condvar and starts with `eof=0`.
- app-created live source records that no stdin reader thread was started.
- `qwen_live_audio_append_s16le()` accepts even-length PCM and rejects invalid input cleanly.
- `qwen_live_audio_append_s16le()` parses little-endian i16 without unaligned `int16_t*` access.
- `qwen_live_audio_finish()` wakes a waiting live transcribe loop.
- `qwen_live_audio_cancel()` wakes a waiting live transcribe loop and causes prompt exit.
- `qwen_live_audio_free()` handles both stdin-created and app-created live sources.
- Rust FFI symbols link on macOS.

### Type / Build Checks

Run after implementation:

```bash
cd openless-all/app
npm run build
```

```bash
cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml local_qwen_transcribe_timeout
cargo test --manifest-path openless-all/app/src-tauri/Cargo.toml streaming_insert_eligible
cargo test --manifest-path openless-all/app/src-tauri/backend-tests/Cargo.toml
```

On macOS, also run the most relevant new targeted tests by exact test names once they exist.

## Manual / App Checks

### Baseline Capture

Before enabling the new live path, record baseline logs on the same machine, same model, same provider, and same sample set:

- active provider: `local-qwen3`
- active model: `qwen3-asr-0.6b`
- model state: warm/cache hit after one preload or first test run
- samples:
  - short: `<= 2s`
  - medium: `5-15s`
  - long: `30-60s`

Capture from logs:

- audio duration
- model load/cache hit
- current ASR elapsed
- LLM polish elapsed
- insert elapsed if available
- stop-to-visible/inserted text if available
- final transcript text

### New Live Path Capture

Run the same sample set after implementation and capture:

- `path=direct_final|live|fallback|failure`
- `live_start_ms_from_recording_start`
- `first_audio_chunk_ms`
- `first_token_ms_from_recording_start`
- `stop_to_final_raw_asr_ms`
- `total_asr_ms`
- `llm_polish_ms`
- `insert_ms`
- `stop_to_visible_or_inserted_text_ms`
- transcript text

Expected:

- short sample uses direct final or live and returns `stop_to_final_raw_asr_ms <= 1.5s`.
- medium sample uses live and returns `stop_to_final_raw_asr_ms <= 3s`.
- long sample uses live and returns `stop_to_final_raw_asr_ms <= 5s`, or is at least 60% faster than baseline.

### Forced Fallback

Implementation should include debug-only validation controls or test hooks to simulate:

- live source create/start failure
- live worker timeout after stop
- live worker does not exit within cancel grace
- live returns empty/invalid final text
- live worker panic/join error where feasible

For each case, verify:

- fallback trigger is logged with reason.
- fallback uses full PCM buffer.
- hard-stall fallback logs `fallback_engine=fresh` if the cached engine remains busy.
- final source is logged as `fallback`.
- user receives normal transcript if fallback succeeds.
- both-path failure displays clear local ASR error.

### Lifecycle Scenarios

Manually verify:

- cancel while recording before live starts
- cancel while live worker is running
- cancel after stop while ASR is finalizing
- quick re-record immediately after prior stop
- release local ASR engine while recording/live worker is active
- release local ASR engine after session completed

Expected:

- no stale token appears in a new session.
- no old transcript is inserted after cancel.
- engine release is deferred or no-ops while busy.
- logs show no stuck live worker after cancel/fallback/session completion.

### Compatibility Scenarios

Verify no behavior change for:

- non-local ASR provider selected
- Apple Speech selected
- QA voice with local-qwen3 selected
- history retranscription
- existing polish modes
- translation mode
- debug audio recording enabled
- streaming insert setting behavior

## Static Searches

Run after implementation:

```bash
rg -n "local-asr-token|LocalAsrTokenPayload|sessionId" openless-all/app/src openless-all/app/src-tauri/src
```

Pass criteria:

- token events are session-aware.
- `CapsulePayload` has optional `sessionId` and visible/processing capsule states publish it.
- frontend does not consume both legacy string and new payload in a way that duplicates text.

```bash
rg -n "qwen_live_audio_start_stdin|qwen_live_audio_create|qwen_transcribe_stream_live" openless-all/app/src-tauri/src openless-all/app/src-tauri/vendor/qwen-asr
```

Pass criteria:

- app live path uses app-created live source, not stdin live reader.
- stdin live reader remains available only for CLI/vendor behavior.

```bash
rg -n "transcribe_stream\\(|set_token_handler" openless-all/app/src-tauri/src/asr/local openless-all/app/src-tauri/src/coordinator
```

Pass criteria:

- fallback final path clears token callback before final transcription.
- no provider accidentally invokes the slow pseudo-stream token path as fallback.
- no-token final path does not add live-only silence padding unless explicitly justified in code comments and excluded from `duration_ms`.

```bash
rg -n "bounded|try_send|overflow|fresh engine|fallback_engine|live_cancel_grace" openless-all/app/src-tauri/src/asr/local openless-all/app/src-tauri/src/coordinator
```

Pass criteria:

- feeder backpressure is explicit and non-blocking.
- hard-stall fallback cannot wait forever on the busy cached engine.

```bash
rg -n "build_local_qwen3|build_local_qwen3_for_dictation|LocalQwenSessionMode" openless-all/app/src-tauri/src/coordinator openless-all/app/src-tauri/src/asr/local
```

Pass criteria:

- normal dictation local-qwen3 uses live-enabled constructor.
- QA and retranscription use batch-only constructor.

## Regression Scenarios

1. Short dictation
   - Record one sentence under 2s.
   - Expect direct final or live, no fallback, stop-to-final raw ASR <= 1.5s.

2. Medium dictation
   - Record 5-15s.
   - Expect live start around first 2s of audio, final within 3s after stop.

3. Long dictation
   - Record 30-60s.
   - Expect live path, final within 5s after stop or >=60% improvement over baseline.

4. Live failure
   - Force live start failure.
   - Expect fallback source, normal transcript if fallback succeeds.

5. Live timeout
   - Force live worker to stall after stop.
   - Expect cancel live, fallback, no worker leak.

6. Live hard-stall
   - Force live worker not to exit within cancel grace.
   - Expect fresh-engine fallback or clear both-path failure if fresh engine cannot load; cached engine remains marked busy until old worker exits.

7. Feeder overflow
   - Force live feeder channel saturation.
   - Expect recorder remains responsive, live path marked unhealthy, final result comes from full-buffer fallback.

8. Invalid live result
   - Force empty/invalid live result.
   - Expect fallback, not empty transcript failure unless fallback also fails.

9. Cancel
   - Cancel during recording and during finalizing.
   - Expect no insertion, no history success row, no stale token.

10. Rapid re-record
   - Start a second recording immediately after stopping/cancelling the first.
   - Expect old tokens/results ignored.

11. Engine release
   - Trigger release while live worker active.
   - Expect release deferred or logged busy, no crash, no ASR failure caused by release.

12. Existing provider
   - Switch to a non-local provider and run a normal dictation.
   - Expect no local live logs and no behavior change.

## Pass Criteria

The implementation passes only if all are true:

- Requirements SLOs are met or exceeded on the agreed short/medium/long samples.
- Logs separate raw ASR, LLM polish, insert, and stop-to-visible timing.
- fallback succeeds in forced live failure cases and is log-only when transcript succeeds.
- live hard-stall cannot block fallback indefinitely; fresh-engine fallback or explicit both-path failure is logged.
- both live and fallback failure produces a clear local ASR error.
- cancel and rapid re-record do not leak worker output into later sessions.
- engine release cannot free or report freed state while a live worker is still using the engine.
- non-local providers and out-of-scope local flows do not change behavior.
- automated checks pass.

## Known Validation Gaps

- CI cannot reliably run full Qwen model inference because model files are large and machine-dependent.
- Performance SLO validation requires the user's macOS machine and warmed local model.
- Audio quality comparison needs human review of transcript equivalence for the short/medium/long sample set.
- QA voice, Less Computer voice, and history retranscription are intentionally out of live-first scope; validation only checks they do not regress.

## Requirements / Technical Coverage Audit

| Requirements area | Technical spec section | Development plan phase | Validation section | Covered |
| --- | --- | --- | --- | --- |
| Fast raw ASR after stop | Proposed Design 4, 5 | Phase 3, Phase 5 | Manual App Checks, Regression 1-3 | Yes |
| Live processing during recording | Proposed Design 4 | Phase 3 | New Live Path Capture | Yes |
| Full PCM fallback | Proposed Design 4 | Phase 3 | Forced Fallback | Yes |
| fallback log-only unless result affected | Proposed Design 4, Error Handling | Phase 3 | Forced Fallback, Pass Criteria | Yes |
| Raw ASR vs final inserted timing | Proposed Design 5 | Phase 1, Phase 4 | Baseline Capture, New Live Path Capture | Yes |
| Short direct/final optimization | Proposed Design 4 | Phase 3 | Regression 1 | Yes |
| Quality/reliability priority | Error Handling and Compatibility | Phase 3, Phase 5 | Manual transcript comparison, Pass Criteria | Yes |
| Recorder callback non-blocking | Proposed Design 4 | Phase 3 | Lifecycle Scenarios, recorder logs | Yes |
| Session safety | Proposed Design 6, Error Handling | Phase 3, Phase 4 | Lifecycle Scenarios, Regression 9-10 | Yes |
| Engine release safety | Proposed Design 7 | Phase 4 | Regression 11 | Yes |
| Hard-stall fallback safety | Proposed Design 4, 7 | Phase 3, Phase 5 | Forced Fallback, Regression 6 | Yes |
| Feeder backpressure safety | Proposed Design 4 | Phase 3 | Automated Checks, Regression 7 | Yes |
| Existing behavior compatibility | Error Handling and Compatibility | Phase 4 | Compatibility Scenarios | Yes |
| First version scope boundary | Session Scope | Phase 4 | Static Searches, Compatibility Scenarios | Yes |
| Baseline and forced fallback validation | Validation plan | Phase 5 | All validation sections | Yes |

Coverage audit result: requirements, technical spec, development plan, and validation plan are aligned. No uncovered requirement remains.
