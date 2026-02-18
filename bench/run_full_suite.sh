#!/usr/bin/env bash
# bench/run_full_suite.sh — Comprehensive Lumen + Cross-Language Benchmark Suite
# Runs all benchmarks (both cross-language and Lumen-specific) with standardized timing.
#
# Benchmarks:
#   Cross-language (C, Go, Rust, Zig, Python, TypeScript, Lumen):
#     - fibonacci: recursive Fibonacci calculation
#     - json_parse: JSON parsing and extraction
#     - string_ops: string manipulation (split, join, replace, etc.)
#     - tree: recursive tree traversal and transformation
#     - sort: array sorting (quicksort)
#     - nbody: N-body gravitational simulation
#     - matrix_mult: matrix multiplication
#     - fannkuch: fannkuch benchmark (cache-busting)
#     - primes_sieve: Sieve of Eratosthenes
#
#   Lumen-specific (b_*.lm files in bench/):
#     - ackermann: Ackermann function (deep recursion)
#     - call_overhead: function call overhead measurement
#     - float_mandelbrot: Mandelbrot set computation
#     - int_fib: optimized integer Fibonacci
#     - int_primes: prime number generation
#     - int_sum_loop: tight integer loop (sum)
#     - list_sum: list folding and summation
#     - string_concat: string concatenation
#
# Usage:
#   bash bench/run_full_suite.sh [--csv output.csv] [--runs N] [--lang LANG] [--no-cross]
#
# Options:
#   --csv FILE       Write results to CSV file
#   --runs N         Number of runs per benchmark (default: 3)
#   --lang LANG      Run only a specific language (c, go, rust, zig, python, typescript, lumen)
#   --no-cross       Skip cross-language benchmarks (run only Lumen-specific)
#   --only-lumen     Run only Lumen (useful for quick testing)
#   -h, --help       Show this help message

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CROSS_DIR="$SCRIPT_DIR/cross-language"
BUILD_DIR="$SCRIPT_DIR/.build"
RESULTS_DIR="$SCRIPT_DIR/results"

RUNS=3
CSV_FILE=""
FILTER_LANG=""
NO_CROSS=false
ONLY_LUMEN=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --csv)        CSV_FILE="$2"; shift 2 ;;
    --runs)       RUNS="$2"; shift 2 ;;
    --lang)       FILTER_LANG="$2"; shift 2 ;;
    --no-cross)   NO_CROSS=true; shift ;;
    --only-lumen) ONLY_LUMEN=true; shift ;;
    -h|--help)
      grep "^#" "$0" | head -40
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

mkdir -p "$BUILD_DIR" "$RESULTS_DIR"

# Detect available compilers
HAS_GCC=false;   command -v gcc &>/dev/null && HAS_GCC=true || true
HAS_GO=false;    command -v go &>/dev/null && HAS_GO=true || true
HAS_PY=false;    command -v python3 &>/dev/null && HAS_PY=true || true
HAS_TS=false;    (command -v npx &>/dev/null || command -v tsx &>/dev/null) && HAS_TS=true || true
HAS_LUMEN=false; (command -v lumen &>/dev/null || [ -f "$REPO_ROOT/target/release/lumen" ]) && HAS_LUMEN=true || true
HAS_RUST=false;  command -v rustc &>/dev/null && HAS_RUST=true || true
HAS_ZIG=false;   command -v zig &>/dev/null && HAS_ZIG=true || true

LUMEN_BIN="lumen"
if ! command -v lumen &>/dev/null && [ -f "$REPO_ROOT/target/release/lumen" ]; then
  LUMEN_BIN="$REPO_ROOT/target/release/lumen"
fi

# Compiler availability summary
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     Lumen Comprehensive Benchmark Suite Runner (v2.0)          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Configuration:"
echo "  Runs per benchmark: $RUNS"
echo "  CSV output: ${CSV_FILE:-none}"
echo "  Filter by language: ${FILTER_LANG:-all}"
echo "  Skip cross-language: $NO_CROSS"
echo "  Only Lumen: $ONLY_LUMEN"
echo ""
echo "Available compilers/interpreters:"
printf "  %-12s %s\n" "gcc:" "$([[ $HAS_GCC = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "go:" "$([[ $HAS_GO = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "rustc:" "$([[ $HAS_RUST = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "zig:" "$([[ $HAS_ZIG = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "python3:" "$([[ $HAS_PY = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "typescript:" "$([[ $HAS_TS = true ]] && echo '✓' || echo '✗')"
printf "  %-12s %s\n" "lumen:" "$([[ $HAS_LUMEN = true ]] && echo '✓' || echo '✗')"
echo ""

# Cross-language benchmarks
CROSS_BENCHMARKS=(
  "fibonacci:fib"
  "json_parse:json_parse"
  "string_ops:string_ops"
  "tree:tree"
  "sort:sort"
  "nbody:nbody"
  "matrix_mult:matrix_mult"
  "fannkuch:fannkuch"
  "primes_sieve:primes_sieve"
)

# Canonical N values per benchmark — all languages MUST use these sizes.
# If a benchmark file uses a different N, results are incomparable.
declare -A CANONICAL_N=(
  [sort]=1000000
  [nbody]=1000000        # steps
  [fibonacci]=35
  [matrix_mult]=200
  [fannkuch]=10
  [primes_sieve]=1000000
  # json_parse, string_ops, tree: N embedded in test data, not a single parameter
)

# Validate that benchmark source files use the canonical N.
# Extracts the primary size parameter from each source file and checks it.
validate_benchmark_sizes() {
  local bench_name="$1"
  local canonical="${CANONICAL_N[$bench_name]:-}"
  if [ -z "$canonical" ]; then
    return 0  # No canonical N defined for this benchmark
  fi

  local bench_dir="$CROSS_DIR/$bench_name"
  local mismatches=()

  for src_file in "$bench_dir"/*; do
    [ -f "$src_file" ] || continue
    local ext="${src_file##*.}"
    local found_n=""

    case "$bench_name" in
      sort)
        found_n=$(grep -oP '(?:let n|const n|n :=|int n|n =)\s*[:=]?\s*([0-9_]+)' "$src_file" \
                  | grep -oP '[0-9_]+$' | head -1 | tr -d '_')
        ;;
      nbody)
        # Look for step count in loop or function call
        found_n=$(grep -oP '(?:for.*0\.\.|(range|<)\s*)([0-9_]+)' "$src_file" \
                  | grep -oP '[0-9_]+' | tail -1 | tr -d '_')
        if [ -z "$found_n" ]; then
          # Lumen uses function argument
          found_n=$(grep -oP 'advance\(.*,\s*([0-9_]+)\s*\)' "$src_file" \
                    | grep -oP '[0-9_]+' | tail -1 | tr -d '_')
        fi
        ;;
      fibonacci)
        found_n=$(grep -oP '(?:fibonacci|fib)\s*\(\s*([0-9]+)' "$src_file" \
                  | grep -oP '[0-9]+' | tail -1)
        ;;
      matrix_mult)
        found_n=$(grep -oP '(?:let n|const N|N :=|#define N|N =)\s*[:=]?\s*([0-9_]+)' "$src_file" \
                  | grep -oP '[0-9_]+$' | head -1 | tr -d '_')
        ;;
      fannkuch)
        found_n=$(grep -oP '(?:let n|const N|N :=|#define N|N =)\s*[:=]?\s*([0-9_]+)' "$src_file" \
                  | grep -oP '[0-9_]+$' | head -1 | tr -d '_')
        ;;
      primes_sieve)
        found_n=$(grep -oP '(?:let limit|const limit|limit :=|int limit|limit =)\s*[:=]?\s*([0-9_]+)' "$src_file" \
                  | grep -oP '[0-9_]+$' | head -1 | tr -d '_')
        ;;
    esac

    if [ -n "$found_n" ] && [ "$found_n" != "$canonical" ]; then
      mismatches+=("$(basename "$src_file"): N=$found_n (expected $canonical)")
    fi
  done

  if [ ${#mismatches[@]} -gt 0 ]; then
    echo "  ⚠ SIZE MISMATCH in $bench_name (canonical N=$canonical):"
    for m in "${mismatches[@]}"; do
      echo "    - $m"
    done
    echo "  Results for mismatched files are NOT comparable!"
    return 1
  fi
  return 0
}

# Lumen-specific benchmarks
LUMEN_BENCHMARKS=(
  "b_ackermann.lm:ackermann"
  "b_call_overhead.lm:call_overhead"
  "b_float_mandelbrot.lm:mandelbrot"
  "b_int_fib.lm:int_fib"
  "b_int_primes.lm:int_primes"
  "b_int_sum_loop.lm:int_sum_loop"
  "b_list_sum.lm:list_sum"
  "b_string_concat.lm:string_concat"
)

# Results array: "benchmark,language,run,time_ms,n"
RESULTS=()

# Time a command, return elapsed milliseconds
time_ms() {
  local start end elapsed
  start=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))' 2>/dev/null || echo "0")
  "$@" > /dev/null 2>&1
  local exit_code=$?
  end=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))' 2>/dev/null || echo "0")
  
  if [ "$start" = "0" ] || [ "$end" = "0" ]; then
    # Fallback: try /usr/bin/time if available
    if command -v /usr/bin/time &>/dev/null; then
      /usr/bin/time -f "%e" "$@" > /dev/null 2>&1
      return $?
    fi
    echo "0"
    return $exit_code
  fi
  
  elapsed=$(( (end - start) / 1000000 ))
  echo "$elapsed"
  return $exit_code
}

run_benchmark() {
  local bench="$1"
  local lang="$2"
  local cmd="$3"
  local n="${CANONICAL_N[$bench]:-}"
  
  # Skip if language filter is set and doesn't match
  if [ -n "$FILTER_LANG" ] && [ "$lang" != "$FILTER_LANG" ]; then
    return
  fi
  
  for run in $(seq 1 "$RUNS"); do
    local ms
    ms=$(time_ms bash -c "$cmd") || ms="ERROR"
    RESULTS+=("$bench,$lang,$run,$ms,$n")
    
    if [ "$ms" = "ERROR" ]; then
      printf "    [%d/%d] %-18s %-12s ERROR\n" "$run" "$RUNS" "$bench" "$lang"
    else
      printf "    [%d/%d] %-18s %-12s %6s ms\n" "$run" "$RUNS" "$bench" "$lang" "$ms"
    fi
  done
}

# ============================================================================
# Cross-Language Benchmarks
# ============================================================================

if [ "$ONLY_LUMEN" = false ] && [ "$NO_CROSS" = false ]; then
  echo "╔════════════════════════════════════════════════════════════════╗"
  echo "║           Cross-Language Benchmarks (9 algorithms)              ║"
  echo "╚════════════════════════════════════════════════════════════════╝"
  echo ""
  
  for bench_spec in "${CROSS_BENCHMARKS[@]}"; do
    IFS=':' read -r bench_name prefix <<< "$bench_spec"
    echo "▶ $bench_name"
    
    # Validate benchmark sizes across languages
    validate_benchmark_sizes "$bench_name" || true
    
    # Log canonical N if defined
    local canonical_n="${CANONICAL_N[$bench_name]:-}"
    if [ -n "$canonical_n" ]; then
      echo "  N=$canonical_n"
    fi
    
    # C
    if $HAS_GCC && [ -f "$CROSS_DIR/$bench_name/${prefix}.c" ]; then
      if gcc -O2 -o "$BUILD_DIR/${bench_name}_c" "$CROSS_DIR/$bench_name/${prefix}.c" -lm 2>/dev/null; then
        run_benchmark "$bench_name" "c" "$BUILD_DIR/${bench_name}_c"
      else
        printf "    [C]      COMPILE ERROR\n"
      fi
    fi
    
    # Go
    if $HAS_GO && [ -f "$CROSS_DIR/$bench_name/${prefix}.go" ]; then
      if go build -o "$BUILD_DIR/${bench_name}_go" "$CROSS_DIR/$bench_name/${prefix}.go" 2>/dev/null; then
        run_benchmark "$bench_name" "go" "$BUILD_DIR/${bench_name}_go"
      else
        printf "    [Go]     COMPILE ERROR\n"
      fi
    fi
    
    # Rust
    if $HAS_RUST && [ -f "$CROSS_DIR/$bench_name/${prefix}.rs" ]; then
      if rustc -O -o "$BUILD_DIR/${bench_name}_rust" "$CROSS_DIR/$bench_name/${prefix}.rs" 2>/dev/null; then
        run_benchmark "$bench_name" "rust" "$BUILD_DIR/${bench_name}_rust"
      else
        printf "    [Rust]   COMPILE ERROR\n"
      fi
    fi
    
    # Zig
    if $HAS_ZIG && [ -f "$CROSS_DIR/$bench_name/${prefix}.zig" ]; then
      if zig build-exe "$CROSS_DIR/$bench_name/${prefix}.zig" -O ReleaseFast -femit-bin="$BUILD_DIR/${bench_name}_zig" 2>/dev/null; then
        run_benchmark "$bench_name" "zig" "$BUILD_DIR/${bench_name}_zig"
      else
        printf "    [Zig]    COMPILE ERROR\n"
      fi
    fi
    
    # Python
    if $HAS_PY && [ -f "$CROSS_DIR/$bench_name/${prefix}.py" ]; then
      run_benchmark "$bench_name" "python" "python3 $CROSS_DIR/$bench_name/${prefix}.py"
    fi
    
    # TypeScript
    if $HAS_TS && [ -f "$CROSS_DIR/$bench_name/${prefix}.ts" ]; then
      if command -v tsx &>/dev/null; then
        run_benchmark "$bench_name" "typescript" "tsx $CROSS_DIR/$bench_name/${prefix}.ts"
      elif command -v npx &>/dev/null; then
        run_benchmark "$bench_name" "typescript" "npx tsx $CROSS_DIR/$bench_name/${prefix}.ts"
      fi
    fi
    
    # Lumen
    if $HAS_LUMEN && [ -f "$CROSS_DIR/$bench_name/${prefix}.lm" ]; then
      run_benchmark "$bench_name" "lumen" "$LUMEN_BIN run $CROSS_DIR/$bench_name/${prefix}.lm"
    fi
    
    echo ""
  done
fi

# ============================================================================
# Lumen-Specific Benchmarks
# ============================================================================

if [ "$ONLY_LUMEN" = true ] || ([ "$NO_CROSS" = false ] && [ "$ONLY_LUMEN" = false ]); then
  echo "╔════════════════════════════════════════════════════════════════╗"
  echo "║        Lumen-Specific Benchmarks (8 language features)          ║"
  echo "╚════════════════════════════════════════════════════════════════╝"
  echo ""
  
  if ! $HAS_LUMEN; then
    echo "⚠ Lumen not available. Skipping Lumen-specific benchmarks."
    echo ""
  else
    for bench_spec in "${LUMEN_BENCHMARKS[@]}"; do
      IFS=':' read -r filename bench_name <<< "$bench_spec"
      echo "▶ $bench_name ($filename)"
      
      run_benchmark "$bench_name" "lumen" "$LUMEN_BIN run $SCRIPT_DIR/$filename"
      echo ""
    done
  fi
fi

# ============================================================================
# Results Summary
# ============================================================================

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Results Summary                             ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Write CSV if requested
if [ -n "$CSV_FILE" ]; then
  echo "benchmark,language,run,time_ms,n" > "$CSV_FILE"
  for row in "${RESULTS[@]}"; do
    echo "$row" >> "$CSV_FILE"
  done
  echo "✓ Raw results written to: $CSV_FILE"
  echo ""
fi

# Generate summary table (median of runs)
echo "Median times (ms, sorted by fastest):"
echo ""

# Build unique benchmark list
declare -A benchmarks_seen
for row in "${RESULTS[@]}"; do
  IFS=',' read -r rb rl rr rt rn <<< "$row"
  benchmarks_seen["$rb"]=1
done

# Build unique language list
declare -A langs_seen
for row in "${RESULTS[@]}"; do
  IFS=',' read -r rb rl rr rt rn <<< "$row"
  langs_seen["$rl"]=1
done

# For each benchmark, show median times sorted
for bench in "${!benchmarks_seen[@]}"; do
  declare -A medians
  
  for lang in "${!langs_seen[@]}"; do
    times=()
    for row in "${RESULTS[@]}"; do
      IFS=',' read -r rb rl rr rt rn <<< "$row"
      if [ "$rb" = "$bench" ] && [ "$rl" = "$lang" ] && [ "$rt" != "ERROR" ]; then
        times+=("$rt")
      fi
    done
    
    if [ ${#times[@]} -gt 0 ]; then
      sorted=($(printf '%s\n' "${times[@]}" | sort -n))
      mid=$(( ${#sorted[@]} / 2 ))
      medians["$lang"]="${sorted[$mid]}"
    else
      medians["$lang"]="-"
    fi
  done
  
  # Sort languages by median time (numeric, skip "-")
  sorted_langs=()
  lang_times=()
  for lang in "${!medians[@]}"; do
    if [ "${medians[$lang]}" != "-" ]; then
      lang_times+=("${medians[$lang]} $lang")
    fi
  done
  
  # Sort numeric, then extract language names
  mapfile -t sorted_lang_times < <(printf '%s\n' "${lang_times[@]}" | sort -n)
  sorted_langs=()
  for entry in "${sorted_lang_times[@]}"; do
    lang="${entry#* }"
    sorted_langs+=("$lang")
  done
  
  # Add any languages with errors at the end
  for lang in "${!medians[@]}"; do
    if [ "${medians[$lang]}" = "-" ]; then
      sorted_langs+=("$lang")
    fi
  done
  
  # Print result line
  printf "  %-24s" "$bench"
  for lang in "${sorted_langs[@]}"; do
    val="${medians[$lang]}"
    if [ "$val" = "-" ]; then
      printf " %-12s" "-"
    else
      printf " %6s ms     " "$val"
    fi
  done
  echo ""
done

echo ""
echo "Language legend: $(printf '%s ' "${!langs_seen[@]}")"
echo ""
echo "✓ Full benchmark suite completed."
echo ""
