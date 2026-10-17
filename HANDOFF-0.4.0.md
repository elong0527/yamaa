# 0.4.0 release drive: handoff for the next session

Written 2026-10-17 12:10 UTC from session
https://claude.ai/code/session_01U6hFXrKrm67peeTGrt7LQm. The canonical
state lives in the roadmap issue #2782 and its comments; this file is the
operator's checklist for resuming. It lives on the working branch only and
must not be merged into main.

## Where main stands (4afa4e0c and later)

Every 0.4.0 code item from the roadmap is merged:

- Track B: #2776 `str_replace` (#2775); #2762 per-template default
  promotion (#2754); #2761 prune every specification (#2696).
- Track C: #2763 structured context and geometry in the adapter record
  (#2675); #2813 named diagnostic records in Python and R (#2676);
  #2789 generated R binding declarations (#2691); #2808 native-lane
  generated-bindings check (#2693).
- Engine: #2798 full-key-tuple lookup index (#2793); #2796 `check`
  reports formula qualification and column-order findings.
- Track A fixes: #2774 Windows memory resolver (#2769); #2810 Windows
  verbatim canonical paths (#2807); #2809 compile fix after #2776 met #2763.
- Hygiene: #2773, #2771, #2777, #2794, #2797, #2811, #2812, #2815.
- Closed without merging: #2767, #2778, #2770, #2779, #2787, #2790.

## Open items, in order

1. **PR #2817** (`claude/fix-rounding-test-context`): one-line compile fix;
   main's workspace test build is red without it (the `#2781` suite decodes
   the issue context as text). Merge when its checks are green. Auto-merge
   is not enabled on the repository.
2. **PR #2816** (owner's bot, `bot/fix-2813-macos-diagnostic-projection`):
   fixes the macOS R lane failure that #2813 carried. Root cause (from the
   diagnostic head): `as.numeric` of a 19-digit token rounds one ulp off on
   arm64 macOS (`R_strtod` without an 80-bit accumulator); the helper
   `record_value_matches` in `R/yamaa/tests/public_contract.R` must match
   wide-integer character tokens against the frame's JSON text, not through
   `as.numeric`. Head cfa7f843 is the bot's fix attempt; merge when green,
   otherwise push that change yourself.
3. **Native qualification (#2783, #2456)**: after #2817 merges, dispatch
   `.github/workflows/native.yml` on main (`actions_run_trigger`, method
   `run_workflow`, `ref: main`) and read every job; runs 1817 to 1821 failed
   only on `native-rust (windows-latest, 3.14)`, which #2810 fixes. Log each
   run on #2783 (one comment per run). The run on the final commit is the
   #2456 evidence.
4. **#2784 docs pass**: branch `claude/release-0.4.0-docs` at e4733427 holds a
   partial draft (`docs/releases/0.4.0.md`, `docs/release.md`,
   `python/README.md`, `rust/README.md`, `rust/interface/README.md`), not
   yet validated. Remaining: `R/yamaa/README.md`, check `docs/articles/schema*.md`
   list `str_replace`, verify every public name against
   `python/src/yamaa/__init__.py` and `R/yamaa/NAMESPACE`, run
   `make docs-test docs` and `make check-repository`, open the PR
   (`Closes #2784`).
5. **#2677 parity suite**: not started (branch `claude/diagnostic-parity`
   has no commits). Design decided on the issue: inventory from the
   `conditions!` registry, authored truth under `rust/tests/host-fixtures/`,
   a committed unreachable-by-design fixture, suites in `make test-python`
   and `make test-r`.
6. **#2457**: owner publication after #2456 is green.
7. **#2792** (`datetime_impute` hour/minute, xjw1001001) conflicts with main
   after #2781; the author's call, not a release gate.

## Working notes

- Two merges this session turned main red the same way: a PR green on its
  own head did not compile against a later base (#2776 and #2781 against
  #2763's structured context). Before merging, update the branch onto main
  and wait for green, or merge one PR at a time and watch main's CI.
- CodeRabbit skips bot-authored PRs (no review seat) and allows one review
  per hour; review bot PRs by reading the diff and merge on green CI.
- GitHub GraphQL is blocked from the session: `gh pr create` fails; use
  `gh api -X POST repos/elong0527/yamaa/pulls --input -` and the MCP tools.
- A push to a branch named `qualification/issue-*` runs `native.yml` on
  that branch (the only way to get Windows and macOS Rust tests on a PR).
- The container has about 40 GB of effective disk. Build with
  `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0`, one worktree per agent
  under `/home/user/yamaa-wt/`, and delete `rust/target` when done.
- `make setup` does not rebuild the Python extension after Rust changes;
  run `uv sync --project python ... --reinstall-package yamaa` first.
