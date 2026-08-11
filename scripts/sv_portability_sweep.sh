#!/usr/bin/env bash
# sv_portability_sweep.sh — multi-frontend SV portability sweep (arch#827 P1/P2).
#
# ARCH's claim to emit portable SystemVerilog was, until this script, asserted
# and never measured: the only SV frontend any CI job ran was Verilator, and
# no CI job ran even that on `arch build` output. This script measures it —
# it builds every `.arch` file in a corpus, parse/elaborate-checks the
# emitted SV with each configured frontend tool, and writes a deterministic
# TSV verdict table. See arch#827 for the investigation that motivated this
# (found 49 designs whose SV no CI job would ever have compiled, 3 of them
# previously-unreported compiler bugs).
#
# ---------------------------------------------------------------------------
# USAGE
#
#   scripts/sv_portability_sweep.sh [OPTIONS] <arch-bin> [<corpus-root>...]
#
# If no corpus roots are given, defaults to: tests examples
#
# <arch-bin>, --out-dir, --baseline, and every <corpus-root> may be given as
# paths relative to the repository root OR as absolute paths — the script
# `cd`s to the repo root first thing, and resolves relative paths from there
# regardless of the caller's cwd. Pass repo-root-relative paths (e.g. `tests`
# `examples`) for baseline portability across machines/checkouts.
#
# OPTIONS
#   -o, --out-dir DIR     Scratch dir for emitted .sv, build/tool logs, and a
#                         throwaway copy of the corpus (see CORPUS COPY
#                         below). Default: a fresh `mktemp -d`.
#   -b, --baseline FILE   Baseline TSV to diff this run against.
#                         Default: tests/portability_baseline.tsv
#   --bless               Do not diff — instead, overwrite (merge into) the
#                         baseline with this run's results. Read the printed
#                         REGRESSION / IMPROVEMENT / CHANGED lines from a
#                         plain (non-bless) run FIRST: blessing hides a real
#                         regression exactly as easily as it records a real
#                         fix. Only bless a diff you have actually reviewed.
#                         Rows for a frontend that was SKIPPED this run
#                         (binary not on PATH) are left untouched in the
#                         baseline rather than deleted, so a partial local
#                         run can never silently drop CI-only coverage.
#   --no-diff             Print the sweep TSV and exit 0; skip the baseline
#                         comparison entirely. Use for the first-ever run
#                         before a baseline exists, or for an ad hoc corpus
#                         subset that was never meant to match the baseline.
#   -j, --jobs N          Parallel workers. Default: 8.
#   -h, --help             Show this help and exit.
#
# EXIT CODES
#   0   sweep completed; no drift vs baseline (or --bless / --no-diff)
#   1   usage error, missing arch-bin, or missing baseline (without --bless
#       or --no-diff)
#   2   drift vs baseline found — see stderr for REGRESSION / IMPROVEMENT /
#       CHANGED lines
#
# ---------------------------------------------------------------------------
# OUTCOME CLASSIFICATION (arch#827's own vocabulary)
#
#   BUILD_FAIL   `arch build ... --no-auto-asserts` itself failed. Expected
#                for negative fixtures (RDC/CDC violation tests, standalone
#                duplicate-def tests, ...) as well as real regressions —
#                the baseline records whichever is true today; the diff
#                still catches a flip either direction.
#   NO_MODULE    `arch build` succeeded but the emitted SV has no top-level
#                `module` to elaborate (package/bus-only file — arch#827's
#                prototype called this PKGONLY). Frontend-independent, so
#                one row, frontend column "-".
#   OK / FAIL    Per configured frontend, on files that DID emit a module:
#                whether that frontend accepted or rejected the SV on a
#                parse/elaborate-only run. One row per (file, frontend).
#   SKIPPED      That frontend's binary isn't on PATH. Reported once (a
#                startup line per missing frontend) and once per affected
#                row, so `--no-diff` output stays self-explanatory; SKIPPED
#                rows are excluded from the baseline diff for that frontend
#                (see --bless above) rather than treated as a verdict.
#
# TSV columns: file<TAB>frontend<TAB>verdict<TAB>reason
#   `reason` is a best-effort, single-line, tab/newline-stripped, 200-char
#   truncated excerpt of the tool's own diagnostic — for a human skimming a
#   diff, not for machine classification (only `verdict` is compared).
#
# ---------------------------------------------------------------------------
# FRONTENDS
#
# The frontend list is the FRONTENDS array below: one string per frontend,
# "name@binary@args@reason-grep-pattern". Adding a new frontend (e.g. slang,
# `yosys -p read_verilog`) is a one-line addition to that array — nothing
# else in the script knows how many frontends there are.
#
# ---------------------------------------------------------------------------
# CORPUS COPY (why this script doesn't build in place)
#
# `arch build` writes `.archi` interface files next to each input (for
# separate compilation) with no flag to suppress or redirect that. Building
# ~900 files in place would scatter hundreds of `.archi` files through
# `tests/`+`examples/` (harmless — `.archi` is gitignored — but messy, and a
# race against anything else touching those trees concurrently, e.g. a
# parallel `cargo test`). `tools/run_arch_regression.py` hit the same issue
# and solved it the same way: copy the corpus into a scratch dir first, build
# there. This script does the same under `<out-dir>/corpus/`.
#
# ---------------------------------------------------------------------------
# macOS note (arch#827): no GNU coreutils `timeout(1)` assumed anywhere here.
# Parallelism deliberately uses `xargs -P -n 1` (each stdin line appended as
# the final argument), NOT `xargs -I{}`: BSD/macOS xargs enforces a small
# (a few hundred byte) cap on the -I replacement string specifically, which
# `<out-dir>/corpus/...` scratch paths blow past easily — measured failure
# ("xargs: command line cannot be assembled, too long") on real corpus paths
# on macOS 15/Darwin 25, GNU xargs unaffected either way. `-n 1` has no such
# limit on either platform. `mktemp -d` is used in its POSIX-common
# no-template form, which both GNU and BSD/macOS accept. Requires bash (the
# shebang); don't source this from zsh.
set -uo pipefail

FRONTENDS=(
  "iverilog@iverilog@-g2012 -gsupported-assertions -t null@(error|sorry|syntax)"
  "verilator@verilator@--lint-only -Wno-fatal --timing@%Error"
  # Add new frontends here, one line each: "name@binary@args@reason-grep"
)

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)/$(basename "${BASH_SOURCE[0]}")"

usage() {
  # Print everything from the second line of this file down to the first
  # line that stops being a leading '#' comment (i.e. the header above).
  awk 'NR==1{next} /^#/{print substr($0,3); next} {exit}' "$SCRIPT_PATH"
}

# ---------------------------------------------------------------- run_one --
# Builds+checks a single .arch file (given as a scratch-copy path under
# "$OUT_DIR/corpus/...") and appends its TSV row(s) to its own rows file.
# Reads ARCH_BIN, OUT_DIR, ARCH_SWEEP_UNAVAILABLE from the environment
# (exported by main() before this is invoked, possibly as a fresh process
# via the "__one" dispatch below).
run_one() {
  local f="$1" orig key sv blog rows reason
  orig="${f#"$OUT_DIR"/corpus/}"
  key="$(printf '%s' "$orig" | tr '/ ' '__')"
  key="${key%.arch}"
  sv="$OUT_DIR/sv/$key.sv"
  blog="$OUT_DIR/log/$key.build.log"
  rows="$OUT_DIR/rows/$key.tsv"
  : > "$rows"

  if ! "$ARCH_BIN" build "$f" -o "$sv" --no-auto-asserts >"$blog" 2>&1; then
    reason="$(grep -m1 '×' "$blog" 2>/dev/null | sed 's/^[[:space:]]*×[[:space:]]*//')"
    [[ -z "$reason" ]] && reason="$(grep -m1 -i 'error' "$blog" 2>/dev/null)"
    [[ -z "$reason" ]] && reason="$(head -n1 "$blog" 2>/dev/null)"
    [[ -z "$reason" ]] && reason="(no output)"
    # Rewrite the throwaway corpus-copy path back to the repo-relative one.
    # `arch` reports the path it was handed, which lives under the per-run
    # scratch dir, so leaving it in makes the baseline row differ on every
    # run and every machine: `--bless` churns it and `git diff` on the
    # baseline is unreadable. (The verdict diff compares column 3 only, so
    # this could never fabricate a REGRESSION — it is reproducibility of
    # the checked-in artifact, not gate correctness.)
    #
    # Matched by shape (`<anything>/corpus/`) rather than by substituting
    # "$OUT_DIR": the two spellings need not agree. On macOS `/tmp` is a
    # symlink to `/private/tmp`, so a caller-supplied `--out-dir /tmp/x`
    # comes back from the compiler as `/private/tmp/x/...` and a literal
    # "$OUT_DIR" replacement silently misses.
    reason="$(printf '%s' "$reason" | sed -E 's#(^|[[:space:]])/[^[:space:]]*/corpus/#\1#g')"
    reason="$(printf '%s' "$reason" | tr '\t\n' '  ' | cut -c1-200)"
    printf '%s\t-\tBUILD_FAIL\t%s\n' "$orig" "$reason" >> "$rows"
    return 0
  fi

  if [[ ! -s "$sv" ]] || ! grep -qE '^module ' "$sv"; then
    printf '%s\t-\tNO_MODULE\t-\n' "$orig" >> "$rows"
    return 0
  fi

  local entry fname fbin fargs fgrep flog
  for entry in "${FRONTENDS[@]}"; do
    IFS='@' read -r fname fbin fargs fgrep <<<"$entry"
    case " $ARCH_SWEEP_UNAVAILABLE " in
      *" $fname "*)
        printf '%s\t%s\tSKIPPED\t%s not installed\n' "$orig" "$fname" "$fbin" >> "$rows"
        continue
        ;;
    esac
    flog="$OUT_DIR/log/$key.$fname.log"
    # shellcheck disable=SC2086  # $fargs is intentionally word-split
    if "$fbin" $fargs "$sv" >"$flog" 2>&1; then
      printf '%s\t%s\tOK\t-\n' "$orig" "$fname" >> "$rows"
    else
      reason="$(grep -m1 -E "$fgrep" "$flog" 2>/dev/null | sed "s#$sv#SV#g")"
      [[ -z "$reason" ]] && reason="$(tail -n1 "$flog" 2>/dev/null)"
      reason="$(printf '%s' "$reason" | tr '\t\n' '  ' | cut -c1-200)"
      printf '%s\t%s\tFAIL\t%s\n' "$orig" "$fname" "$reason" >> "$rows"
    fi
  done
}

# ---------------------------------------------------------- worker dispatch
# A fresh invocation of this same script, spawned per-file by xargs, so
# run_one() never has to fight bash's `export -f` portability quirks across
# GNU/BSD xargs. Must come after run_one() is defined, before main() runs.
if [[ "${1:-}" == "__one" ]]; then
  shift
  run_one "$1"
  exit 0
fi

# Prepass worker: build only, for the `.archi` side effect. No rows, no
# frontend checks, failures ignored — a file that can't build yet may build
# on a later round once a dependency's interface exists. See the prepass
# block in main() for why this is needed at all.
if [[ "${1:-}" == "__prepass" ]]; then
  shift
  "$ARCH_BIN" build "$1" -o /dev/null --no-auto-asserts >/dev/null 2>&1 || true
  exit 0
fi

# ------------------------------------------------------------ diff_baseline
# Compares $2 (this run's TSV) against $1 (baseline TSV) by (file,frontend)
# key. Prints REGRESSION/IMPROVEMENT/CHANGED/SKIPPED lines to stderr and a
# SUMMARY line; returns 1 if anything but SKIPPED drifted, else 0.
#
# New keys (file/frontend pairs absent from the baseline — corpus growth)
# never fail: a fixture added by this branch has nothing to be compared to.
#
# A *vanished* key is only benign when the file left the corpus. If the file
# is still swept but no longer has that (file,frontend) row, its row SHAPE
# changed, which is how a build regression looks: a design that emitted a
# module and was checked per frontend (`f OK` x N rows) now fails to build
# and produces a single `f - BUILD_FAIL` row instead. Keying purely on
# (file,frontend) counted that as `removed` + `new` and passed green, so
# `arch build` breaking a corpus design was invisible to this gate. Such a
# key is now classified REGRESSION (baseline OK) or CHANGED (baseline not
# OK) and fails, while a key whose file genuinely left the corpus is still
# just reported.
diff_baseline() {
  local baseline="$1" current="$2"
  awk -F'\t' '
    NR==FNR {
      if (FNR==1) next
      key=$1 FS $2
      base[key]=$3
      next
    }
    FNR==1 { next }
    {
      key=$1 FS $2
      cur[key]=$3
      seen[key]=1
      curfile[$1]=1
      # A per-frontend row (frontend column != "-") means `arch build`
      # succeeded for this file this run.
      if ($2 != "-") filebuilt[$1]=1
    }
    END {
      regress=0; improve=0; changed=0; skipped=0; new=0; removed=0
      for (k in seen) {
        if (!(k in base)) { new++; continue }
        if (base[k] == cur[k]) continue
        if (cur[k] == "SKIPPED") {
          printf "SKIPPED\t%s\t(frontend unavailable this run; baseline=%s)\n", k, base[k] > "/dev/stderr"
          skipped++
        } else if (base[k] == "OK" && cur[k] != "OK") {
          printf "REGRESSION\t%s\t%s -> %s\n", k, base[k], cur[k] > "/dev/stderr"
          regress++
        } else if (base[k] != "OK" && cur[k] == "OK") {
          printf "IMPROVEMENT\t%s\t%s -> %s\n", k, base[k], cur[k] > "/dev/stderr"
          improve++
        } else {
          printf "CHANGED\t%s\t%s -> %s\n", k, base[k], cur[k] > "/dev/stderr"
          changed++
        }
      }
      for (k in base) {
        if (k in seen) continue
        f = substr(k, 1, index(k, FS) - 1)
        if (!(f in curfile)) { removed++; continue }
        # File still swept, but this (file,frontend) row is gone: its row
        # SHAPE changed. Which direction depends on what replaced it —
        # `filebuilt` is set when the file produced any per-frontend row
        # this run, i.e. `arch build` succeeded and a frontend judged it.
        if (base[k] == "OK") {
          printf "REGRESSION\t%s\t%s -> (row gone; file no longer builds)\n", k, base[k] > "/dev/stderr"
          regress++
        } else if (f in filebuilt) {
          # e.g. BUILD_FAIL -> per-frontend rows: the design started
          # building. Genuinely good news, but it still fails the diff so
          # a human re-blesses deliberately rather than the baseline
          # drifting on its own.
          printf "IMPROVEMENT\t%s\t%s -> (row gone; file now builds)\n", k, base[k] > "/dev/stderr"
          improve++
        } else {
          printf "CHANGED\t%s\t%s -> (row gone; file now has a different row shape)\n", k, base[k] > "/dev/stderr"
          changed++
        }
      }
      printf "SUMMARY\tregressions=%d improvements=%d changed=%d skipped=%d new=%d removed=%d\n", \
        regress, improve, changed, skipped, new, removed > "/dev/stderr"
      exit (regress > 0 || improve > 0 || changed > 0) ? 1 : 0
    }
  ' "$baseline" "$current"
}

# ------------------------------------------------------------ bless_baseline
# Merges $2 (this run's TSV) over $1 (baseline path, created empty if it
# doesn't exist yet). For every (file,frontend) key this run actually
# produced a non-SKIPPED verdict for, the fresh row wins; every other
# baseline row (untouched files, or a frontend SKIPPED this run) survives
# unchanged. Writes the merged, sorted result back to $1.
bless_baseline() {
  local baseline="$1" current="$2" before after
  mkdir -p "$(dirname "$baseline")"
  [[ -f "$baseline" ]] || : > "$baseline"
  before="$(wc -l < "$baseline" | tr -d ' ')"
  {
    printf 'file\tfrontend\tverdict\treason\n'
    awk -F'\t' '
      NR==FNR {
        if (FNR==1) next
        key=$1 FS $2
        base_line[key]=$0
        next
      }
      FNR==1 { next }
      {
        key=$1 FS $2
        if ($3 != "SKIPPED") {
          cur_line[key]=$0
          cur_seen[key]=1
        }
      }
      END {
        for (k in base_line) {
          print (k in cur_seen) ? cur_line[k] : base_line[k]
        }
        for (k in cur_line) {
          if (!(k in base_line)) print cur_line[k]
        }
      }
    ' "$baseline" "$current" | LC_ALL=C sort -t "$(printf '\t')" -k1,1 -k2,2
  } > "$baseline.tmp"
  mv "$baseline.tmp" "$baseline"
  after="$(wc -l < "$baseline" | tr -d ' ')"
  echo "sv-portability-sweep: blessed $baseline ($before -> $after rows) from $current" >&2
}

# ------------------------------------------------------------------- main --
main() {
  local ARCH_BIN_ARG="" OUT_DIR_ARG="" BASELINE="tests/portability_baseline.tsv"
  local MODE="diff" JOBS=8
  local -a ROOTS=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      -o|--out-dir) OUT_DIR_ARG="$2"; shift 2 ;;
      -b|--baseline) BASELINE="$2"; shift 2 ;;
      --bless) MODE="bless"; shift ;;
      --no-diff) MODE="no-diff"; shift ;;
      -j|--jobs) JOBS="$2"; shift 2 ;;
      -h|--help) usage; exit 0 ;;
      --) shift; break ;;
      -*)
        echo "sv-portability-sweep: unknown option: $1" >&2
        usage >&2
        exit 1
        ;;
      *)
        if [[ -z "$ARCH_BIN_ARG" ]]; then
          ARCH_BIN_ARG="$1"
        else
          ROOTS+=("$1")
        fi
        shift
        ;;
    esac
  done
  # Remaining args after `--` are corpus roots too.
  while [[ $# -gt 0 ]]; do
    ROOTS+=("$1")
    shift
  done

  if [[ -z "$ARCH_BIN_ARG" ]]; then
    echo "sv-portability-sweep: missing <arch-bin>" >&2
    usage >&2
    exit 1
  fi

  local repo_root
  repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
    echo "sv-portability-sweep: not inside a git repo" >&2
    exit 1
  }

  # Resolve arch-bin to an absolute path BEFORE cd'ing, since it may be
  # relative to the caller's cwd (e.g. ./target/release/arch).
  case "$ARCH_BIN_ARG" in
    /*) ARCH_BIN="$ARCH_BIN_ARG" ;;
    *) ARCH_BIN="$(cd "$(dirname "$ARCH_BIN_ARG")" >/dev/null 2>&1 && pwd)/$(basename "$ARCH_BIN_ARG")" ;;
  esac
  if [[ ! -x "$ARCH_BIN" ]]; then
    echo "sv-portability-sweep: arch-bin not executable: $ARCH_BIN_ARG" >&2
    echo "  (see doc/../CLAUDE.md \"never invoke a stale compiler binary\" — did you \`cargo build --release\`?)" >&2
    exit 1
  fi

  cd "$repo_root" || exit 1

  [[ "${#ROOTS[@]}" -eq 0 ]] && ROOTS=(tests examples)
  local root
  for root in "${ROOTS[@]}"; do
    if [[ ! -d "$root" ]]; then
      echo "sv-portability-sweep: corpus root not found: $root" >&2
      exit 1
    fi
  done

  local OUT_DIR
  if [[ -n "$OUT_DIR_ARG" ]]; then
    mkdir -p "$OUT_DIR_ARG"
    OUT_DIR="$(cd "$OUT_DIR_ARG" && pwd)"
  else
    OUT_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t arch-portability-sweep)"
  fi
  mkdir -p "$OUT_DIR/corpus" "$OUT_DIR/sv" "$OUT_DIR/log" "$OUT_DIR/rows"

  echo "sv-portability-sweep: copying corpus root(s) into $OUT_DIR/corpus (see CORPUS COPY in --help)" >&2
  for root in "${ROOTS[@]}"; do
    mkdir -p "$OUT_DIR/corpus/$(dirname "$root")"
    cp -R "$root" "$OUT_DIR/corpus/$root"
  done

  # Drop `.archi` files the copy inherited from the working tree unless git
  # tracks them. `.archi` is gitignored, so a developer tree accumulates
  # them from every earlier `arch build` — 153 of them in the tree this was
  # first measured on — while a CI checkout has only the handful that are
  # committed. Since a stale `.archi` is exactly what auto-discovery
  # consumes, leaving them in makes local results disagree with CI and makes
  # neither reproducible. The prepass below regenerates whatever is really
  # needed, deterministically.
  if git -C . rev-parse --git-dir >/dev/null 2>&1; then
    local tracked_archi
    tracked_archi="$(git -C . ls-files '*.archi' 2>/dev/null || true)"
    while IFS= read -r stale; do
      [[ -z "$stale" ]] && continue
      local rel="${stale#"$OUT_DIR"/corpus/}"
      case $'\n'"$tracked_archi"$'\n' in
        *$'\n'"$rel"$'\n'*) ;;   # committed fixture — keep
        *) rm -f "$stale" ;;
      esac
    done < <(find "$OUT_DIR/corpus" -name '*.archi' -print)
  fi

  find "$OUT_DIR/corpus" -name '*.arch' -print | LC_ALL=C sort -u > "$OUT_DIR/files.list"
  local nfiles
  nfiles="$(wc -l < "$OUT_DIR/files.list" | tr -d ' ')"
  if [[ "$nfiles" -eq 0 ]]; then
    echo "sv-portability-sweep: no .arch files found under: ${ROOTS[*]}" >&2
    exit 1
  fi

  local entry fname fbin fargs fgrep
  local unavailable=""
  for entry in "${FRONTENDS[@]}"; do
    IFS='@' read -r fname fbin fargs fgrep <<<"$entry"
    if ! command -v "$fbin" >/dev/null 2>&1; then
      unavailable="$unavailable $fname"
      echo "sv-portability-sweep: frontend '$fname' ($fbin) not found on PATH — all rows for it will be recorded as SKIPPED and excluded from the baseline diff this run." >&2
    fi
  done

  export ARCH_BIN OUT_DIR
  export ARCH_SWEEP_UNAVAILABLE="$unavailable"

  # ---------------------------------------------------------------- prepass
  # Populate `.archi` interface files before measuring anything.
  #
  # `arch build` auto-discovers an undefined `inst` target from a sibling
  # `<Name>.archi`, and emits those stubs as a side effect of building. In
  # one shared corpus tree that makes a design's verdict depend on how many
  # of its dependencies happened to be built first: `tests/aes/
  # aes_key_expand_mod.arch` reports `undefined module: AesSbox` when built
  # before `aes_sbox.arch`, gets one dependency further once `AesSbox.archi`
  # exists, and eventually builds outright. Scheduling therefore decided the
  # verdict — measured divergence between `-j 8` locally and `-j 4` on CI —
  # which would make any gate built on this sweep flap (arch#887's second
  # defect).
  #
  # So: build everything first, discarding results, and repeat until the set
  # of emitted `.archi` stops growing (a dependency chain resolves one level
  # per pass). Every file in the measured pass then sees the same interface
  # set no matter what order or `-j` the runner picked. Capped at 4 rounds
  # so a pathological corpus can't spin; the cap is reported when hit,
  # because silently measuring a not-yet-converged tree is the failure mode
  # this whole block exists to prevent.
  local round prev_count archi_count
  prev_count=-1
  for round in 1 2 3 4; do
    archi_count="$(find "$OUT_DIR/corpus" -name '*.archi' | wc -l | tr -d ' ')"
    [[ "$archi_count" -eq "$prev_count" ]] && break
    prev_count="$archi_count"
    echo "sv-portability-sweep: prepass $round (interfaces: $archi_count)" >&2
    xargs -P "$JOBS" -n 1 "$SCRIPT_PATH" __prepass < "$OUT_DIR/files.list"
  done
  archi_count="$(find "$OUT_DIR/corpus" -name '*.archi' | wc -l | tr -d ' ')"
  if [[ "$archi_count" -ne "$prev_count" ]]; then
    echo "sv-portability-sweep: WARNING interface set still growing after 4 prepasses ($prev_count -> $archi_count); verdicts may not be order-stable" >&2
  fi
  rm -f "$OUT_DIR"/rows/*.tsv 2>/dev/null || true

  echo "sv-portability-sweep: sweeping $nfiles file(s) from [${ROOTS[*]}] -> $OUT_DIR (jobs=$JOBS)" >&2
  # -n 1 (one stdin line appended per invocation), deliberately NOT -I{} —
  # see the macOS note in the header comment above.
  xargs -P "$JOBS" -n 1 "$SCRIPT_PATH" __one < "$OUT_DIR/files.list"

  {
    printf 'file\tfrontend\tverdict\treason\n'
    cat "$OUT_DIR"/rows/*.tsv 2>/dev/null | LC_ALL=C sort -t "$(printf '\t')" -k1,1 -k2,2 -k3,3
  } > "$OUT_DIR/portability.tsv"
  echo "sv-portability-sweep: wrote $OUT_DIR/portability.tsv ($(( $(wc -l < "$OUT_DIR/portability.tsv") - 1 )) row(s))" >&2

  case "$MODE" in
    bless)
      bless_baseline "$BASELINE" "$OUT_DIR/portability.tsv"
      ;;
    no-diff)
      cat "$OUT_DIR/portability.tsv"
      ;;
    diff)
      if [[ ! -f "$BASELINE" ]]; then
        echo "sv-portability-sweep: baseline not found: $BASELINE" >&2
        echo "  run with --bless to create it, or --no-diff to skip comparison." >&2
        exit 1
      fi
      if ! diff_baseline "$BASELINE" "$OUT_DIR/portability.tsv"; then
        echo "" >&2
        echo "sv-portability-sweep: drift detected vs $BASELINE (see lines above)." >&2
        echo "  REGRESSION/CHANGED must be fixed. Genuine IMPROVEMENT: review, then re-run with --bless." >&2
        exit 2
      fi
      echo "sv-portability-sweep: no drift vs $BASELINE" >&2
      ;;
  esac
}

main "$@"
