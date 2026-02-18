#!/usr/bin/env bash
# bench/run_all.sh — Cross-language benchmark runner
# Compiles and runs each benchmark in each language, records wall-clock time.
# Usage: bash bench/run_all.sh [--csv output.csv] [--runs N]
#
# Requires: gcc, go, python3, npx (for ts-node/tsx), zig, cargo (for Lumen)
# Missing compilers are skipped gracefully.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CROSS_DIR="$SCRIPT_DIR/cross-language"
BUILD_DIR="$SCRIPT_DIR/.build"

RUNS=3
CSV_FILE=""

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --csv)    CSV_FILE="$2"; shift 2 ;;
    --runs)   RUNS="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--csv output.csv] [--runs N]"
      echo "  --csv FILE   Write results to CSV file"
      echo "  --runs N     Number of runs per benchmark (default: 3)"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

mkdir -p "$BUILD_DIR"

# Detect available compilers
HAS_GCC=false; command -v gcc &>/dev/null && HAS_GCC=true
HAS_GO=false;  command -v go  &>/dev/null && HAS_GO=true
HAS_PY=false;  command -v python3 &>/dev/null && HAS_PY=true
HAS_TS=false;  (command -v npx &>/dev/null || command -v tsx &>/dev/null) && HAS_TS=true
HAS_LUMEN=false; (command -v lumen &>/dev/null || [ -f "$REPO_ROOT/target/release/lumen" ]) && HAS_LUMEN=true
HAS_RUST=false;  command -v rustc &>/dev/null && HAS_RUST=true
HAS_ZIG=false;   command -v zig &>/dev/null && HAS_ZIG=true

LUMEN_BIN="lumen"
if ! command -v lumen &>/dev/null && [ -f "$REPO_ROOT/target/release/lumen" ]; then
  LUMEN_BIN="$REPO_ROOT/target/release/lumen"
fi

echo "=== Cross-Language Benchmark Runner ==="
echo "Runs per benchmark: $RUNS"
echo "Compilers: gcc=$HAS_GCC go=$HAS_GO rust=$HAS_RUST zig=$HAS_ZIG python3=$HAS_PY ts=$HAS_TS lumen=$HAS_LUMEN"
echo ""

BENCHMARKS=("fibonacci" "json_parse" "string_ops" "tree" "sort")

# File mapping: benchmark -> filename prefix
declare -A FILE_MAP=(
  [fibonacci]="fib"
  [json_parse]="json_parse"
  [string_ops]="string_ops"
  [tree]="tree"
  [sort]="sort"
)

# Results array: "benchmark,language,run,time_ms"
RESULTS=()

# Time a command, return elapsed milliseconds
time_ms() {
  local start end elapsed
  start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
  "$@" > /dev/null 2>&1
  local exit_code=$?
  end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
  elapsed=$(( (end - start) / 1000000 ))
  echo "$elapsed"
  return $exit_code
}

run_benchmark() {
  local bench="$1"
  local lang="$2"
  local cmd="$3"
  
  for run in $(seq 1 "$RUNS"); do
    local ms
    ms=$(time_ms bash -c "$cmd") || ms="ERROR"
    RESULTS+=("$bench,$lang,$run,$ms")
    if [ "$ms" = "ERROR" ]; then
      printf "  %-12s %-10s run %d: ERROR\n" "$bench" "$lang" "$run"
    else
      printf "  %-12s %-10s run %d: %s ms\n" "$bench" "$lang" "$run" "$ms"
    fi
  done
}

for bench in "${BENCHMARKS[@]}"; do
  prefix="${FILE_MAP[$bench]}"
  echo "--- $bench ---"

  # C
  if $HAS_GCC && [ -f "$CROSS_DIR/$bench/$prefix.c" ]; then
    gcc -O2 -o "$BUILD_DIR/${bench}_c" "$CROSS_DIR/$bench/$prefix.c" -lm 2>/dev/null && \
      run_benchmark "$bench" "c" "$BUILD_DIR/${bench}_c" || \
      echo "  $bench c: COMPILE ERROR"
  fi

  # Go
  if $HAS_GO && [ -f "$CROSS_DIR/$bench/$prefix.go" ]; then
    go build -o "$BUILD_DIR/${bench}_go" "$CROSS_DIR/$bench/$prefix.go" 2>/dev/null && \
      run_benchmark "$bench" "go" "$BUILD_DIR/${bench}_go" || \
      echo "  $bench go: COMPILE ERROR"
  fi

  # Rust
  if $HAS_RUST && [ -f "$CROSS_DIR/$bench/$prefix.rs" ]; then
    rustc -O -o "$BUILD_DIR/${bench}_rust" "$CROSS_DIR/$bench/$prefix.rs" 2>/dev/null && \
      run_benchmark "$bench" "rust" "$BUILD_DIR/${bench}_rust" || \
      echo "  $bench rust: COMPILE ERROR"
  fi

  # Zig
  if $HAS_ZIG && [ -f "$CROSS_DIR/$bench/$prefix.zig" ]; then
    zig build-exe "$CROSS_DIR/$bench/$prefix.zig" -O ReleaseFast -femit-bin="$BUILD_DIR/${bench}_zig" 2>/dev/null && \
      run_benchmark "$bench" "zig" "$BUILD_DIR/${bench}_zig" || \
      echo "  $bench zig: COMPILE ERROR"
  fi

  # Python
  if $HAS_PY && [ -f "$CROSS_DIR/$bench/$prefix.py" ]; then
    run_benchmark "$bench" "python" "python3 $CROSS_DIR/$bench/$prefix.py"
  fi

  # TypeScript (via tsx or ts-node)
  if $HAS_TS && [ -f "$CROSS_DIR/$bench/$prefix.ts" ]; then
    if command -v tsx &>/dev/null; then
      run_benchmark "$bench" "typescript" "tsx $CROSS_DIR/$bench/$prefix.ts"
    elif command -v npx &>/dev/null; then
      run_benchmark "$bench" "typescript" "npx tsx $CROSS_DIR/$bench/$prefix.ts"
    fi
  fi

  # Lumen
  if $HAS_LUMEN && [ -f "$CROSS_DIR/$bench/$prefix.lm" ]; then
    run_benchmark "$bench" "lumen" "$LUMEN_BIN run $CROSS_DIR/$bench/$prefix.lm"
  fi

  echo ""
done

# Write CSV if requested
if [ -n "$CSV_FILE" ]; then
  echo "benchmark,language,run,time_ms" > "$CSV_FILE"
  for row in "${RESULTS[@]}"; do
    echo "$row" >> "$CSV_FILE"
  done
  echo "Results written to $CSV_FILE"
fi

# Print summary table (median of runs)
echo "=== Summary (median of $RUNS runs, in ms) ==="
printf "%-14s" "benchmark"
LANGS=("c" "go" "rust" "zig" "python" "typescript" "lumen")
for lang in "${LANGS[@]}"; do
  printf "%-12s" "$lang"
done
echo ""

for bench in "${BENCHMARKS[@]}"; do
  printf "%-14s" "$bench"
  for lang in "${LANGS[@]}"; do
    # Collect times for this bench+lang
    times=()
    for row in "${RESULTS[@]}"; do
      IFS=',' read -r rb rl rr rt <<< "$row"
      if [ "$rb" = "$bench" ] && [ "$rl" = "$lang" ] && [ "$rt" != "ERROR" ]; then
        times+=("$rt")
      fi
    done
    if [ ${#times[@]} -eq 0 ]; then
      printf "%-12s" "-"
    else
      # Sort and take median
      sorted=($(printf '%s\n' "${times[@]}" | sort -n))
      mid=$(( ${#sorted[@]} / 2 ))
      printf "%-12s" "${sorted[$mid]}"
    fi
  done
  echo ""
done

echo ""
echo "Done."
