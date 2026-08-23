#!/usr/bin/env bash
# macos-qwen-stream-ab.sh — vendored Qwen3-ASR 流式解码的 A/B 回归护栏。
#
# 背景：vendored 库把 `past_text_conditioning` 默认成 0（qwen_asr.c 的
# qwen_load），但那个默认值只对 segmented 路径成立；CLI 在 --stream 下会强制
# 置 1（main.c）。App 曾经因为没接这根线，长期跑在关闭状态下，导致流式转写
# 出现「中间重复整段」与「中段/尾部吞字」。
#
# 本脚本把当时用来定位和验证的 A/B 固化下来，每次 submodule bump 后重跑，
# 防止配置再次漂回默认值。
#
#   REF = --silent                      整段离线解码，参照系
#   APP = --stream --past-text no       历史上出问题的配置
#   FIX = --stream --past-text yes      期望配置（App 现在应等价于此）
#
# 判定门槛（来自 2026-08 实测基线）：FIX 档必须 hit_max_new == 0、
# recovery_reset == 0，且 emitted_total 轨迹单调不减。
#
# 用法：
#   scripts/macos-qwen-stream-ab.sh
#   scripts/macos-qwen-stream-ab.sh --model-dir /path/to/qwen3-asr-0.6b
#   scripts/macos-qwen-stream-ab.sh --keep          # 保留构建产物和输出便于排查

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="${SCRIPT_DIR}/../src-tauri/vendor/qwen-asr"
DEFAULT_MODEL_DIR="${HOME}/Library/Application Support/OpenLess/models/qwen3-asr/qwen3-asr-0.6b"

MODEL_DIR="${DEFAULT_MODEL_DIR}"
KEEP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model-dir) MODEL_DIR="$2"; shift 2 ;;
    --keep)      KEEP=1; shift ;;
    -h|--help)   sed -n '2,25p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "未知参数: $1" >&2; exit 2 ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "本脚本只在 macOS 上有意义（本地 Qwen ASR 是 macOS-only）。" >&2
  exit 2
fi

if [[ ! -f "${VENDOR_DIR}/qwen_asr.c" ]]; then
  echo "找不到 vendored 源码：${VENDOR_DIR}" >&2
  echo "submodule 没有 checkout？试试 git submodule update --init --recursive" >&2
  exit 2
fi

if [[ ! -f "${MODEL_DIR}/model.safetensors" && ! -f "${MODEL_DIR}/model.safetensors.index.json" ]]; then
  echo "找不到模型：${MODEL_DIR}" >&2
  echo "用 --model-dir 指定，或先在 App 里下载本地模型。" >&2
  exit 2
fi

WORK_DIR="$(mktemp -d -t qwen-stream-ab)"
cleanup() {
  if [[ "${KEEP}" -eq 1 ]]; then
    echo
    echo "产物保留在：${WORK_DIR}"
  else
    rm -rf "${WORK_DIR}"
  fi
}
trap cleanup EXIT

echo "==> 复制 vendored 源码到 ${WORK_DIR}/src（不污染仓库）"
cp -R "${VENDOR_DIR}" "${WORK_DIR}/src"

echo "==> 编译 CLI（make blas / Accelerate）"
make -C "${WORK_DIR}/src" blas >"${WORK_DIR}/build.log" 2>&1 || {
  echo "编译失败，日志：" >&2
  tail -30 "${WORK_DIR}/build.log" >&2
  exit 1
}

BIN="${WORK_DIR}/src/qwen_asr"
SAMPLES="${WORK_DIR}/src/samples/night_of_the_living_dead_1968"
OUT="${WORK_DIR}/out"
mkdir -p "${OUT}"

# 选带 ground-truth .txt 的定长片段。10s 以下不进入本护栏——问题只在长录音上
# 显现（<10s 桶历史截断率为 0%）。
CLIPS=(
  "10s_back_down_the_road"
  "21s_hey_thats_us_were_doing_all_right"
  "30s_i_wont_open_this_door_again"
  "45s_dont_be_afraid_of_me"
)

echo "==> 跑 A/B（${#CLIPS[@]} 个片段 × 3 档，需要几分钟）"
for clip in "${CLIPS[@]}"; do
  wav="${SAMPLES}/${clip}.wav"
  if [[ ! -f "${wav}" ]]; then
    echo "  跳过缺失样本：${clip}" >&2
    continue
  fi
  echo "  - ${clip}"
  "${BIN}" -d "${MODEL_DIR}" -i "${wav}" --silent \
      >"${OUT}/${clip}.ref.txt" 2>"${OUT}/${clip}.ref.err"
  "${BIN}" -d "${MODEL_DIR}" -i "${wav}" --stream --past-text no  --debug \
      >"${OUT}/${clip}.app.txt" 2>"${OUT}/${clip}.app.err"
  "${BIN}" -d "${MODEL_DIR}" -i "${wav}" --stream --past-text yes --debug \
      >"${OUT}/${clip}.fix.txt" 2>"${OUT}/${clip}.fix.err"
done

echo
python3 - "${OUT}" "${SAMPLES}" "${CLIPS[@]}" <<'PY'
import re, sys, unicodedata
from pathlib import Path

out_dir = Path(sys.argv[1])
samples = Path(sys.argv[2])
clips = sys.argv[3:]

def normalize(s):
    s = unicodedata.normalize("NFKC", s).lower()
    s = re.sub(r"[^\w\s]", " ", s)
    return re.sub(r"\s+", " ", s).strip()

def levenshtein(a, b):
    if a == b:
        return 0
    if not a or not b:
        return max(len(a), len(b))
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]

def counters(err_path):
    txt = err_path.read_text(errors="replace")
    commits = [int(m) for m in re.findall(r"emitted_total=(\d+)", txt)]
    return {
        "hit_max": len(re.findall(r"hit max_new", txt)),
        "reset": len(re.findall(r"Recovery reset applied", txt)),
        "commits": commits,
    }

def monotonic(seq):
    return all(b >= a for a, b in zip(seq, seq[1:]))

def repeated_span(s, minlen=5, window=60):
    n = len(s)
    for L in range(min(30, n // 2), minlen - 1, -1):
        for i in range(n - L + 1):
            sub = s[i:i + L]
            j = s.find(sub, i + 1)
            if j != -1 and j - i <= window:
                return sub
    return None

print(f"{'clip':<38} {'cfg':<4} {'hit_max':>8} {'reset':>6} {'mono':>5} {'norm_dist':>10}  dup")
print("-" * 100)

failures = []
for clip in clips:
    ref_txt = samples / f"{clip}.txt"
    reference = normalize(ref_txt.read_text()) if ref_txt.exists() else None
    for cfg in ("ref", "app", "fix"):
        txt_path = out_dir / f"{clip}.{cfg}.txt"
        err_path = out_dir / f"{clip}.{cfg}.err"
        if not txt_path.exists():
            continue
        text = txt_path.read_text().strip()
        c = counters(err_path)
        dist = "-"
        if reference:
            hyp = normalize(text)
            dist = f"{levenshtein(hyp, reference) / max(1, len(reference)):.3f}"
        mono = "-" if not c["commits"] else ("yes" if monotonic(c["commits"]) else "NO")
        dup = repeated_span(text)
        print(f"{clip:<38} {cfg:<4} {c['hit_max']:>8} {c['reset']:>6} {mono:>5} {dist:>10}  "
              f"{repr(dup) if dup else ''}")

        if cfg == "fix":
            if c["hit_max"]:
                failures.append(f"{clip}: FIX 触顶 max_new {c['hit_max']} 次（应为 0）")
            if c["reset"]:
                failures.append(f"{clip}: FIX 触发 recovery reset {c['reset']} 次（应为 0）")
            if c["commits"] and not monotonic(c["commits"]):
                failures.append(f"{clip}: FIX 的 emitted_total 轨迹非单调 {c['commits']}")
    print()

if failures:
    print("FAIL —— 流式配置疑似回归到 past_text_conditioning=0：")
    for f in failures:
        print(f"  - {f}")
    sys.exit(1)

print("PASS —— FIX 档未触顶 max_new、未触发 recovery reset、提交轨迹单调。")
PY
