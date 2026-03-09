#!/usr/bin/env bash
# benchmarks/run.sh — Lumen benchmark runner vs Python 3 and Node.js
# Usage: ./benchmarks/run.sh [--jit|--no-jit] [bench_name]
#
# Requires: python3, node (optional), cargo (for Lumen)
# Missing runtimes are skipped gracefully.
#
# Exit code: 0 if all Lumen benchmarks succeed, 1 if any fail.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Runtime detection ─────────────────────────────────────────────────────────
JIT_FLAG=""
BENCH_FILTER=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --jit)     JIT_FLAG="--jit"; shift ;;
    --no-jit)  JIT_FLAG="--no-jit"; shift ;;
    -h|--help)
      echo "Usage: $0 [--jit|--no-jit] [bench_name]"
      echo "  --jit       Force Lumen JIT on"
      echo "  --no-jit    Force Lumen JIT off"
      echo "  bench_name  Run only this benchmark (fib|nbody|sort|strings)"
      exit 0 ;;
    *) BENCH_FILTER="$1"; shift ;;
  esac
done

# Locate Lumen binary
LUMEN_BIN=""
if [ -f "$REPO_ROOT/target/release/lumen" ]; then
  LUMEN_BIN="$REPO_ROOT/target/release/lumen"
elif command -v lumen &>/dev/null; then
  LUMEN_BIN="lumen"
fi

PY="${PY:-python3}"
NODE="${NODE:-node}"

HAS_LUMEN=false; [ -n "$LUMEN_BIN" ] && HAS_LUMEN=true
HAS_PY=false;    command -v "$PY" &>/dev/null && HAS_PY=true
HAS_NODE=false;  command -v "$NODE" &>/dev/null && HAS_NODE=true

# ── Colors ────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'

# ── Timing helper (returns ms as integer) ────────────────────────────────────
time_ms() {
  local start end elapsed
  start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
  eval "$*" > /dev/null 2>&1
  local ec=$?
  end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
  elapsed=$(( (end - start) / 1000000 ))
  echo "$elapsed"
  return $ec
}

LUMEN_FAILURES=0

# ── Benchmark runner ──────────────────────────────────────────────────────────
run_bench() {
  local name="$1" bench_dir="$2"
  local lumen_file="$bench_dir/main.lm.md"
  local py_file="$bench_dir/main.py"
  local js_file="$bench_dir/main.js"

  echo -e "${BLUE}=== $name ===${NC}"

  # Lumen
  if $HAS_LUMEN && [ -f "$lumen_file" ]; then
    local lumen_cmd="$LUMEN_BIN run $JIT_FLAG $lumen_file"
    local ms
    if ms=$(time_ms "$lumen_cmd" 2>/dev/null); then
      echo -e "  ${GREEN}Lumen  ${NC}: ${ms}ms"
    else
      echo -e "  ${RED}Lumen  ${NC}: FAILED (exit non-zero)"
      LUMEN_FAILURES=$((LUMEN_FAILURES + 1))
    fi
  else
    echo -e "  ${YELLOW}Lumen  ${NC}: (binary not found — run: cargo build --release --bin lumen)"
  fi

  # Python
  if $HAS_PY && [ -f "$py_file" ]; then
    local ms
    if ms=$(time_ms "$PY $py_file" 2>/dev/null); then
      echo -e "  ${GREEN}Python ${NC}: ${ms}ms"
    else
      echo -e "  ${YELLOW}Python ${NC}: FAILED"
    fi
  fi

  # Node
  if $HAS_NODE && [ -f "$js_file" ]; then
    local ms
    if ms=$(time_ms "$NODE $js_file" 2>/dev/null); then
      echo -e "  ${GREEN}Node   ${NC}: ${ms}ms"
    else
      echo -e "  ${YELLOW}Node   ${NC}: FAILED"
    fi
  fi

  echo ""
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "=== Lumen Benchmark Suite ==="
echo "Lumen: ${LUMEN_BIN:-'(not built)'}"
echo "Python: $($HAS_PY && $PY --version 2>&1 || echo 'not found')"
echo "Node: $($HAS_NODE && $NODE --version 2>&1 || echo 'not found')"
echo "JIT: ${JIT_FLAG:-'(default)'}"
echo ""

declare -A BENCH_DIRS=(
  [fib]="$SCRIPT_DIR/fib"
  [nbody]="$SCRIPT_DIR/nbody"
  [sort]="$SCRIPT_DIR/sort"
  [strings]="$SCRIPT_DIR/strings"
)

declare -A BENCH_NAMES=(
  [fib]="fibonacci(35)"
  [nbody]="nbody(50000 steps)"
  [sort]="mergesort(100k ints)"
  [strings]="string-processing(10k)"
)

for key in fib nbody sort strings; do
  if [ -n "$BENCH_FILTER" ] && [ "$BENCH_FILTER" != "$key" ]; then
    continue
  fi
  run_bench "${BENCH_NAMES[$key]}" "${BENCH_DIRS[$key]}"
done

if [ $LUMEN_FAILURES -gt 0 ]; then
  echo -e "${RED}$LUMEN_FAILURES Lumen benchmark(s) failed.${NC}"
  exit 1
else
  echo -e "${GREEN}All Lumen benchmarks ran successfully.${NC}"
  exit 0
fi
