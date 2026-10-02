#!/bin/sh
# factory.sh -- scaffolding and mechanical admission for the yamaa benchmark
# factory (see README.md). POSIX sh; needs only python3, git, and uv
# (the repo's own runner). Stages 3 (Harbor pilot) is intentionally out
# of scope: it needs Docker and a model key.
#
# Usage:
#   factory.sh scaffold <name>    create benchmarks/<name>/ skeleton
#   factory.sh validate <name>    run the mechanical admission (Stage 2)
#   factory.sh packet <name>      print a Stage 4/5 review packet as markdown
set -u

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BENCH="$ROOT/benchmarks"
PY="uv run --project $ROOT/python --no-sync"

die() { echo "factory.sh: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "need '$1' on PATH"; }

name="${2:-}"
case "${1:-}" in
  scaffold|validate|packet) ;;
  *) die "usage: factory.sh {scaffold|validate|packet} <benchmark-name>" ;;
esac
[ -n "$name" ] || die "benchmark name required"
case "$name" in
  *[!a-z0-9-]*|"" ) die "name must be lowercase letters, digits, hyphens" ;;
esac
DIR="$BENCH/$name"

# ---------------------------------------------------------------- scaffold
if [ "$1" = "scaffold" ]; then
  [ -e "$DIR" ] && die "$DIR already exists"
  mkdir -p "$DIR/input" "$DIR/expected"
  if [ "${name#negative-}" != "$name" ]; then NEG=1; else NEG=0; fi
  {
    echo "# <Title: what the benchmark derives>"
    echo
    echo "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/$name.html)"
    echo "[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)"
    echo
    echo "**Goal:** <the variables derived, in study-data words>."
    echo
    echo "**Input:** <the source shape, in plain words>."
    echo
    echo "**Variables:**"
    echo
    echo "- \`<VAR>\` is <what its value means>; <what it holds when the inputs do not support it>."
    echo
    echo "**Standard:** <SDTM|ADaM> | **Domain:** <DM|AE|ADSL|...>"
    if [ "$NEG" = 1 ]; then
      echo
      echo "## How to fix"
      echo
      echo "<Lead with the clinical or data decision, then the smallest valid correction.>"
    fi
  } > "$DIR/README.md"
  echo "scaffolded $DIR"
  echo "next: write spec.yaml, input/*.csv, expected/*, then: factory.sh validate $name"
  exit 0
fi

# --------------------------------------------------------------- validate
if [ "$1" = "validate" ]; then
  need python3
  [ -d "$DIR" ] || die "no such benchmark: $DIR"
  FAIL=0
  say() { printf '%s\n' "$2"; [ "$1" = "ok" ] || FAIL=1; }
  skip() { printf 'SKIP: %s\n' "$1"; }
  HAVE_UV=0
  command -v uv >/dev/null 2>&1 && HAVE_UV=1

  # 1. layout: spec, fixtures, runners
  if [ "${name#negative-}" != "$name" ]; then
    [ -f "$DIR/expected/error.yaml" ] && say ok "error.yaml present" \
      || say bad "negative benchmark needs expected/error.yaml"
    for f in phase condition spec_paths requirement; do
      grep -q "^$f:" "$DIR/expected/error.yaml" 2>/dev/null \
        && say ok "error.yaml has $f" || say bad "error.yaml missing $f"
    done
  else
    ls "$DIR"/spec*.yaml >/dev/null 2>&1 \
      && say ok "spec file(s) present" || say bad "no spec*.yaml"
    ls "$DIR"/expected/*.csv >/dev/null 2>&1 \
      && say ok "expected csv present" || say bad "no expected/*.csv"
    [ -f "$DIR/run.py" ] && [ -f "$DIR/run.R" ] \
      && say ok "run.py and run.R present" \
      || say bad "positive benchmark needs run.py and run.R"
  fi

  # 2. README badges
  grep -q "Lifecycle: draft\|Lifecycle: reviewed\|Lifecycle: finalized" \
      "$DIR/README.md" 2>/dev/null \
    && say ok "lifecycle badge present" || say bad "README missing lifecycle badge"
  grep -q "Dashboard" "$DIR/README.md" 2>/dev/null \
    && say ok "dashboard badge present" || say bad "README missing dashboard badge"

  # 3. README data-contract rules (ported from benchmarks/agents.md)
  OUT="$(cd "$BENCH" && python3 - "$name" <<'PY'
import re, sys
name = sys.argv[1]
f = f"{name}/README.md"
text = open(f).read()
pattern = re.compile(r"R0[0-9][0-9]|output\.columns|handler|verification")
contract = text.split("\n## How to fix\n", 1)[0]
hits = [f"{i}: {l}" for i, l in enumerate(contract.splitlines(), 1)
        if pattern.search(l)]
if hits:
    print("schema vocabulary in data contract:")
    print("\n".join(hits))
if name.startswith("negative-"):
    n = text.count("\n## How to fix\n")
    if n != 1:
        print(f"expected exactly one '## How to fix', found {n}")
PY
)"
  [ -z "$OUT" ] && say ok "README contract clean" || { say bad "README contract:"; echo "$OUT"; }

  # 4. every non-key golden column described in the README
  OUT="$(cd "$BENCH" && python3 - "$name" <<'PY'
import glob, os, sys
name = sys.argv[1]
KEYS = {"STUDYID", "USUBJID", "DOMAIN", "SUBJID", "AESEQ", "VSSEQ",
        "LBSEQ", "RSSEQ", "ASEQ", "PARAMCD", "PARAM", "AVISIT", "VISIT",
        "RDOMAIN", "IDVAR", "QNAM"}
for f in sorted(glob.glob(f"{name}/expected/*.csv")):
    rd = open(f"{name}/README.md").read()
    cols = open(f).readline().strip().split(",")
    missing = [c for c in cols if c not in rd and c not in KEYS]
    if missing:
        print(f, "->", missing)
PY
)"
  [ -z "$OUT" ] && say ok "golden columns described" || { say bad "undescribed columns:"; echo "$OUT"; }

  # 5. spec style contract (needs uv; skip when unavailable)
  if ls "$DIR"/spec*.yaml >/dev/null 2>&1; then
    if [ "$HAVE_UV" = 0 ]; then
      skip "spec style check needs uv"
    # shellcheck disable=SC2086
    elif (cd "$ROOT" && $PY python -m yamaa.style "$DIR"/spec*.yaml) >/dev/null 2>&1; then
      say ok "spec style clean"
    else
      say bad "spec style violations (run: yamaa.style --fix)"
    fi
  fi

  # 6. repo-wide validators (prose gate + column labels; needs uv)
  if [ "$HAVE_UV" = 0 ]; then
    skip "validate_repository.py needs uv"
  elif (cd "$BENCH" && $PY python "$ROOT/.github/scripts/yaml-validation/validate_repository.py") \
      >/tmp/factory-validate.log 2>&1; then
    say ok "validate_repository.py clean"
  else
    say bad "validate_repository.py failed (see /tmp/factory-validate.log)"
  fi

  [ "$FAIL" = 0 ] && echo "PASS: $name admitted to Stage 3" \
    || { echo "FAIL: $name needs work before Stage 3"; exit 1; }
  exit 0
fi

# ----------------------------------------------------------------- packet
# packet: assemble everything a Stage 4 judge or Stage 5 human needs.
need git
[ -d "$DIR" ] || die "no such benchmark: $DIR"
{
  echo "# Review packet: \`$name\`"
  echo
  echo "Generated $(date -u +%Y-%m-%dT%H:%M:%SZ) from $(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unversioned)."
  echo
  echo "## Files"
  echo
  (cd "$DIR" && find . -type f | sort | sed 's/^/- `/;s/$/`/')
  echo
  echo "## README.md"
  echo
  cat "$DIR/README.md"
  echo
  for s in "$DIR"/spec*.yaml; do
    [ -f "$s" ] || continue
    echo "## $(basename "$s")"
    echo
    echo '```yaml'
    cat "$s"
    echo '```'
    echo
  done
  for f in "$DIR"/input/*.csv; do
    [ -f "$f" ] || continue
    echo "## input/$(basename "$f") (first 15 rows)"
    echo
    echo '```csv'
    head -15 "$f"
    echo '```'
    echo
  done
  if [ -f "$DIR/expected/error.yaml" ]; then
    echo "## expected/error.yaml"
    echo
    echo '```yaml'
    cat "$DIR/expected/error.yaml"
    echo '```'
  else
    for f in "$DIR"/expected/*.csv; do
      [ -f "$f" ] || continue
      echo "## expected/$(basename "$f") (first 15 rows)"
      echo
      echo '```csv'
      head -15 "$f"
      echo '```'
      echo
    done
  fi
  echo "## Verdict (fill per judges.md)"
  echo
  echo '```'
  echo "Benchmark: $name (revision <n>)"
  echo "1. Construct validity: pass | revise | fail -- <evidence>"
  echo "2. Correctness:        pass | revise | fail -- <evidence>"
  echo "3. Feasibility:        pass | revise | fail -- <evidence>"
  echo "4. Usefulness:         pass | revise | fail -- <evidence>"
  echo "5. No leakage:         pass | revise | fail -- <evidence>"
  echo "Verdict: accept | revise | reject"
  echo 'Next: <the single most important change, or "ready for the human gate">'
  echo '```'
}
