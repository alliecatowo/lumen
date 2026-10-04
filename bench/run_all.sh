#!/usr/bin/env bash
# bench/run_all.sh — Cross-language benchmark runner
#
# Compiles and runs every program under bench/cross-language/<bench>/ in each
# available language, records wall-clock time per run, and CHECKS THE OUTPUT of
# every language against bench/cross-language/<bench>/expected.txt (the Python
# reference output, compared case-insensitively). A run whose output is wrong or
# whose process fails is recorded as ERROR/WRONG and excluded from the medians,
# so a crash or a wrong answer can never show up as a "fast" time.
#
# Usage: bash bench/run_all.sh [--csv output.csv] [--runs N] [--only a,b,c]
#                              [--lumen /path/to/lumen] [--interp]
#
#   --csv FILE      Write per-run results to FILE and environment metadata to
#                   FILE with a .meta suffix (commit, CPU, versions, date).
#   --runs N        Runs per benchmark and language (default 3).
#   --only LIST     Comma-separated benchmark names (default: all 9).
#   --lumen PATH    Lumen binary (default: $LUMEN_BIN, then ./target/release/lumen,
#                   then `lumen` on PATH).
#   --interp        Also run Lumen with the JIT disabled (LUMEN_JIT=0) and report it
#                   as the language `lumen-interp`.
#
# Notes on what is measured:
#   * Lumen samples are `lumen run <file>`: process start-up, compilation and
#     execution. The compiler is fast, but this is not execution time alone.
#   * Every sample is the wall-clock time of the whole process.
#
# Missing compilers/interpreters are skipped. Needs python3 (timing + checking).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CROSS_DIR="$SCRIPT_DIR/cross-language"
BUILD_DIR="$SCRIPT_DIR/.build"

RUNS=3
CSV_FILE=""
ONLY=""
WITH_INTERP=false
LUMEN_BIN="${LUMEN_BIN:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --csv)    CSV_FILE="$2"; shift 2 ;;
    --runs)   RUNS="$2"; shift 2 ;;
    --only)   ONLY="$2"; shift 2 ;;
    --lumen)  LUMEN_BIN="$2"; shift 2 ;;
    --interp) WITH_INTERP=true; shift ;;
    -h|--help)
      sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

mkdir -p "$BUILD_DIR"

if [ -z "$LUMEN_BIN" ]; then
  if [ -x "$REPO_ROOT/target/release/lumen" ]; then
    LUMEN_BIN="$REPO_ROOT/target/release/lumen"
  elif command -v lumen &>/dev/null; then
    LUMEN_BIN="$(command -v lumen)"
  fi
fi

HAS_GCC=false;   command -v gcc     &>/dev/null && HAS_GCC=true
HAS_GO=false;    command -v go      &>/dev/null && go version &>/dev/null && HAS_GO=true
HAS_PY=false;    command -v python3 &>/dev/null && HAS_PY=true
HAS_TS=false;    (command -v tsx &>/dev/null || command -v npx &>/dev/null) && HAS_TS=true
HAS_LUMEN=false; [ -n "$LUMEN_BIN" ] && [ -x "$LUMEN_BIN" ] && HAS_LUMEN=true
HAS_RUST=false;  command -v rustc   &>/dev/null && HAS_RUST=true
HAS_ZIG=false;   command -v zig     &>/dev/null && HAS_ZIG=true

if ! $HAS_PY; then
  echo "python3 is required (timing and output checking)"; exit 1
fi

ALL_BENCHMARKS=(fibonacci json_parse string_ops tree sort nbody fannkuch matrix_mult primes_sieve)
if [ -n "$ONLY" ]; then
  IFS=',' read -r -a BENCHMARKS <<< "$ONLY"
else
  BENCHMARKS=("${ALL_BENCHMARKS[@]}")
fi

# benchmark -> source file prefix
prefix_of() {
  case "$1" in
    fibonacci) echo fib ;;
    *)         echo "$1" ;;
  esac
}

echo "=== Cross-Language Benchmark Runner ==="
echo "Benchmarks: ${BENCHMARKS[*]}"
echo "Runs per benchmark: $RUNS"
echo "Tools: gcc=$HAS_GCC go=$HAS_GO rust=$HAS_RUST zig=$HAS_ZIG python3=$HAS_PY ts=$HAS_TS lumen=$HAS_LUMEN${LUMEN_BIN:+ ($LUMEN_BIN)}"
echo ""

# Results: "benchmark,language,run,time_ms" (time_ms may be ERROR or WRONG)
RESULTS=()

# time_cmd <outfile> <cmd...>: run the command, store stdout in outfile and print
# "<elapsed_ms> <exit_code>".
time_cmd() {
  local out="$1"; shift
  python3 - "$out" "$@" <<'PY'
import subprocess, sys, time
out = sys.argv[1]
cmd = sys.argv[2:]
start = time.perf_counter()
try:
    with open(out, "wb") as f:
        rc = subprocess.run(cmd, stdout=f, stderr=subprocess.DEVNULL).returncode
except OSError:
    rc = 127
print(int((time.perf_counter() - start) * 1000), rc)
PY
}

# check_output <actual> <expected>: every expected line must appear, in order, in
# the (ANSI-stripped, lower-cased) actual output. Lumen's CLI prints status lines
# around the program output, which is why this is a subsequence check.
check_output() {
  python3 - "$1" "$2" <<'PY'
import re, sys
ansi = re.compile(r"\x1b\[[0-9;]*m")
actual = [ansi.sub("", l).strip().lower() for l in open(sys.argv[1], errors="replace")]
expected = [l.strip().lower() for l in open(sys.argv[2]) if l.strip()]
i = 0
for line in actual:
    if i < len(expected) and line == expected[i]:
        i += 1
sys.exit(0 if i == len(expected) else 1)
PY
}

run_benchmark() {
  local bench="$1" lang="$2" cmd="$3" expected="$4"
  local out="$BUILD_DIR/${bench}_${lang}.out"
  local run ms rc
  for run in $(seq 1 "$RUNS"); do
    read -r ms rc < <(eval "time_cmd \"$out\" $cmd")
    if [ "$rc" != "0" ]; then
      ms="ERROR"
    elif [ -f "$expected" ] && ! check_output "$out" "$expected"; then
      ms="WRONG"
    fi
    RESULTS+=("$bench,$lang,$run,$ms")
    case "$ms" in
      ERROR) printf "  %-12s %-12s run %d: ERROR (exit %s)\n" "$bench" "$lang" "$run" "$rc" ;;
      WRONG) printf "  %-12s %-12s run %d: WRONG OUTPUT (see %s)\n" "$bench" "$lang" "$run" "$out" ;;
      *)     printf "  %-12s %-12s run %d: %s ms\n" "$bench" "$lang" "$run" "$ms" ;;
    esac
  done
}

for bench in "${BENCHMARKS[@]}"; do
  prefix="$(prefix_of "$bench")"
  dir="$CROSS_DIR/$bench"
  expected="$dir/expected.txt"
  if [ ! -d "$dir" ]; then
    echo "--- $bench --- (missing directory $dir, skipped)"; continue
  fi
  echo "--- $bench ---"

  if $HAS_GCC && [ -f "$dir/$prefix.c" ]; then
    if gcc -O2 -o "$BUILD_DIR/${bench}_c" "$dir/$prefix.c" -lm 2>/dev/null; then
      run_benchmark "$bench" "c" "\"$BUILD_DIR/${bench}_c\"" "$expected"
    else
      echo "  $bench c: COMPILE ERROR"
    fi
  fi

  if $HAS_GO && [ -f "$dir/$prefix.go" ]; then
    if go build -o "$BUILD_DIR/${bench}_go" "$dir/$prefix.go" 2>/dev/null; then
      run_benchmark "$bench" "go" "\"$BUILD_DIR/${bench}_go\"" "$expected"
    else
      echo "  $bench go: COMPILE ERROR"
    fi
  fi

  if $HAS_RUST && [ -f "$dir/$prefix.rs" ]; then
    if rustc -O -o "$BUILD_DIR/${bench}_rust" "$dir/$prefix.rs" 2>/dev/null; then
      run_benchmark "$bench" "rust" "\"$BUILD_DIR/${bench}_rust\"" "$expected"
    else
      echo "  $bench rust: COMPILE ERROR"
    fi
  fi

  if $HAS_ZIG && [ -f "$dir/$prefix.zig" ]; then
    if zig build-exe "$dir/$prefix.zig" -O ReleaseFast -femit-bin="$BUILD_DIR/${bench}_zig" 2>/dev/null; then
      run_benchmark "$bench" "zig" "\"$BUILD_DIR/${bench}_zig\"" "$expected"
    else
      echo "  $bench zig: COMPILE ERROR"
    fi
  fi

  if $HAS_PY && [ -f "$dir/$prefix.py" ]; then
    run_benchmark "$bench" "python" "python3 \"$dir/$prefix.py\"" "$expected"
  fi

  if $HAS_TS && [ -f "$dir/$prefix.ts" ]; then
    if command -v tsx &>/dev/null; then
      run_benchmark "$bench" "typescript" "tsx \"$dir/$prefix.ts\"" "$expected"
    else
      run_benchmark "$bench" "typescript" "npx tsx \"$dir/$prefix.ts\"" "$expected"
    fi
  fi

  if $HAS_LUMEN && [ -f "$dir/$prefix.lm" ]; then
    run_benchmark "$bench" "lumen" "\"$LUMEN_BIN\" run \"$dir/$prefix.lm\"" "$expected"
    if $WITH_INTERP; then
      run_benchmark "$bench" "lumen-interp" "env LUMEN_JIT=0 \"$LUMEN_BIN\" run \"$dir/$prefix.lm\"" "$expected"
    fi
  fi

  echo ""
done

# --- Metadata ---------------------------------------------------------------
cpu_model() {
  if [ -r /proc/cpuinfo ]; then
    awk -F': ' '/model name/ {print $2; exit}' /proc/cpuinfo
  elif command -v sysctl &>/dev/null; then
    sysctl -n machdep.cpu.brand_string 2>/dev/null
  else
    echo unknown
  fi
}
write_meta() {
  local f="$1"
  {
    echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "repo_commit: $(git -C "$REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
    echo "repo_dirty: $(git -C "$REPO_ROOT" status --porcelain 2>/dev/null | grep -q . && echo yes || echo no)"
    echo "lumen: $($HAS_LUMEN && "$LUMEN_BIN" --version 2>/dev/null | head -1 || echo none)"
    echo "lumen_binary: ${LUMEN_BIN:-none}"
    echo "cpu: $(cpu_model)"
    echo "cores: $(getconf _NPROCESSORS_ONLN 2>/dev/null || echo unknown)"
    echo "os: $(uname -srm)"
    echo "runs: $RUNS"
    echo "gcc: $($HAS_GCC && gcc --version | head -1 || echo none)"
    echo "rustc: $($HAS_RUST && rustc --version || echo none)"
    echo "go: $($HAS_GO && go version || echo none)"
    echo "python: $($HAS_PY && python3 --version || echo none)"
  } > "$f"
}

if [ -n "$CSV_FILE" ]; then
  echo "benchmark,language,run,time_ms" > "$CSV_FILE"
  for row in "${RESULTS[@]}"; do
    echo "$row" >> "$CSV_FILE"
  done
  write_meta "${CSV_FILE}.meta"
  echo "Results written to $CSV_FILE (metadata: ${CSV_FILE}.meta)"
fi

# --- Summary (median of correct runs) ---------------------------------------
echo "=== Summary (median of correct runs, in ms; - = no correct run) ==="
LANGS=("c" "go" "rust" "zig" "python" "typescript" "lumen")
$WITH_INTERP && LANGS+=("lumen-interp")
printf "%-14s" "benchmark"
for lang in "${LANGS[@]}"; do printf "%-14s" "$lang"; done
echo ""

for bench in "${BENCHMARKS[@]}"; do
  printf "%-14s" "$bench"
  for lang in "${LANGS[@]}"; do
    times=()
    bad=0
    for row in "${RESULTS[@]}"; do
      IFS=',' read -r rb rl rr rt <<< "$row"
      if [ "$rb" = "$bench" ] && [ "$rl" = "$lang" ]; then
        if [ "$rt" = "ERROR" ] || [ "$rt" = "WRONG" ]; then bad=$((bad + 1)); else times+=("$rt"); fi
      fi
    done
    if [ ${#times[@]} -eq 0 ]; then
      if [ "$bad" -gt 0 ]; then printf "%-14s" "FAILED"; else printf "%-14s" "-"; fi
    else
      sorted=($(printf '%s\n' "${times[@]}" | sort -n))
      mid=$(( ${#sorted[@]} / 2 ))
      cell="${sorted[$mid]}"
      [ "$bad" -gt 0 ] && cell="$cell(${bad}bad)"
      printf "%-14s" "$cell"
    fi
  done
  echo ""
done

echo ""
echo "Done."
