# yamaa agent guide

This file is the single entry point for agents. It maps the repository,
states the hard rules and the design principles, and says what to run
before a push. Area guides (linked below) add detail; none of them
overrides this file. Release docs are not needed for normal changes: read
`docs/release.md` only for a release, a supported-platform change, or a
migration decision.

Run `make help` at the repository root for the supported checks and
their prerequisites. Local targets and CI share the owned scripts under
`tools/commands/`.

## Start here

The whole of this guide is the contract; this list is the order of work.

1. Read the issue, its "Order" line and its latest comments. Assign
   yourself to it, or link your pull request on it; that is the claim.
2. Create a worktree from `origin/main` (below) and point it at the
   machine's shared Cargo target.
3. Build the change, in its data form first
   ([Design principles](#design-principles)), with the tests nearest it,
   and nothing else.
4. Run the local checks of [Before a push](#before-a-push) once, then push
   once, with the explicit refspec. Open the pull request ready for review.
5. Get CI green on the exact head, answer its reviews, and stop. The
   owner's maintenance session merges.
6. Record a decision only where [Decisions](#decisions) says to.

## Repo map

- `.github/` -- CI workflows, issue forms and pull request configuration.
- `automation/` -- the benchmark factory and other maintainer automation, with its own guides.
- `benchmarks/` -- one directory per benchmark: spec, inputs, and authored expected output; see `benchmarks/AGENTS.md`.
- `docs/` -- site pages and release support; `CONTRIBUTING.md` owns development guidance.
- `evaluations/` -- the Harbor agent evaluation: prompts, solutions, grading, and leaderboards.
- `python/` -- the Python host package: a thin facade over the shared Rust engine.
- `R/` -- the R host package under `R/yamaa/`, likewise a facade over the shared engine.
- `rules/` -- the normative language contracts, indexed by `rules/README.md`.
- `rust/` -- the shared Rust engine workspace, fixtures and installed-host qualification.
- `tools/` -- importable repository checks, documentation, staging and conformance tools.
- `yaml/` -- the specification schemas, closed grammars, and conformance fixtures; see `yaml/README.md`.

## Hard rules

- **Preserve maintainer automation.** Files under `automation/` are
  owner-maintained material. Do not modify, delete, rename, move or replace
  them unless the owner explicitly requests that specific change. General
  cleanup, consolidation, refactoring or an assessment that content is stale
  does not authorize changing these files. Report concerns to the owner and
  leave the files intact.
- **Breaking a user-facing interface is a decision.** Until 1.0.0, a change
  that breaks the specification language, the Python or R API, saved outputs
  or diagnostic identity follows a decision recorded on #3030 or an owner
  decision on its own issue. It gets a one-line entry in the next release's
  notes under `docs/releases/` and no step-by-step migration text. Internal
  interfaces, including the unpublished Rust crates, may change freely,
  except the `yamaa` facade's public items, frozen from 1.0.0. Remove
  obsolete interfaces and callers together instead of adding compatibility
  aliases. From 1.0.0, the stability contract in `docs/release.md` applies.
- **Complete review before merging.** The owner's maintenance session
  reviews and merges every pull request (D175 on #3030), only once CI is
  green on the exact head (D176) and one completed review of that head is
  on the pull request: CodeRabbit's, Codex's, or the maintenance session's
  own, recorded as `## Review of <full sha>`. Every actionable finding is
  resolved first. A pending, skipped or older-head review does not count.
  The author never reviews its own pull request (D148). `CONTRIBUTING.md`
  holds the gate's mechanics and what replaces an unavailable reviewer.
- **Expected data is truth.** Files under `benchmarks/**/expected/` are
  independently authored and define what an implementation must
  reproduce. Never regenerate them from the implementation, and never
  edit `benchmarks/**/input/` or `benchmarks/**/expected/` to make a
  failing implementation pass. Changing a golden file is a decision:
  reproduce it independently first, and say in the pull request which
  values moved and why.
- **Write the project name as lowercase `yamaa`**, never `YAMAA` --
  including at the start of a sentence, in headings, page titles, nav
  labels, and doc comments. One exception: `YAMAA-01` inside
  `benchmarks/**/input/` and `benchmarks/**/expected/` is a CDISC
  `STUDYID` data value, not the project name written as prose.
- **Label the benchmark runtime `opensas`** in benchmark labels, prose,
  prompts, commands, dataset names, leaderboards, and solution comments.
  Keep the standard `.sas` source extension, upstream asset names, and
  required legal trademark attribution intact.
- **No content hashing.** The repository does not hash content: a
  runtime artifact is pinned by `runtime.artifact.reference` alone, a
  contract fingerprint is its canonical RFC 8785 JSON, the activation
  cache key is the joined identities themselves, and a resource snapshot
  compares the bytes it already holds. Do not reintroduce SHA-256 (or
  any digest field) to give something an identity. The packaging-tool
  exceptions are `python/uv.lock`, `evaluations/harbor/uv.lock` and
  `rust/Cargo.lock`, including unchanged or trimmed copies staged into
  Python/R source packages and the `.cargo-checksum.json` files of their
  vendored crates; their checksum fields belong to the packaging tools.
- **Site pages have one source.** Edit `README.md` for the site home
  page and `rules/README.md` for the rules index. MkDocs reads those
  files through `tools/docs/canonical_pages.py`; do not create
  `docs/index.md` or `docs/articles/rules.md`.

## Design principles

The hard rules say what never changes; these say how the engine stays
small while it grows. `CONTRIBUTING.md#design-principles-in-practice`
holds the mechanics: which data form owns which behaviour, the fixture
homes and their readers, and how to measure the ceilings.

- **Behaviour is data first.** A new behaviour goes into the data form
  the repository already owns before any Rust: a schema module or grammar
  file under `yaml/`, the condition registry, the vocabulary, a
  conformance case or a benchmark, the host interface in `bindings.yaml`.
  Rust holds semantics and execution. When no form can express the
  behaviour, extend the form once, with its generator or check, rather
  than writing the behaviour by hand where the form's consumers would
  have read it.
- **One implementation per judgement.** Never add a second code path that
  must agree with an existing one: a check that mirrors binding, a
  validator that mirrors a decoder, a hand copy of a generated list. Put
  the judgement behind one abstraction with two sources, or generate both
  copies from one declaration and check them equal in
  `make check-repository`.
- **A fixture before a test function.** A case of the form "files or
  documents, one call, the expected rows or bytes" is a fixture in an
  owned home (a benchmark, a `rust/tests/host-fixtures/` family, a
  crate's `tests/fixtures/`), read by that home's reader. A Rust, Python
  or R test function proves an internal seam the readers cannot reach,
  and its first comment names the seam.
- **Generate from one declaration; keep data as data.** When two artifacts
  describe one thing, one is the source and the other is generated from
  it and checked current. Data stays in its own format, a copied file
  behind `include_str!`, not a generated source file of string constants.
- **Size has a ceiling.** A source or test file splits by feature before
  3,000 lines (#2413). A function over 250 lines, or with more than seven
  parameters, is a context struct or a split waiting to happen; Clippy
  fails both. An `allow` for either names the issue that removes it.
  Generated files are exempt from the file ceiling, not from being data.
  Growth is measured per crate and recorded on the planning issue, never
  in committed files.

## Pull requests

- **One theme per pull request**, as small as its theme allows (D171 on
  #3030, amended by D230). One issue is the usual size; group issues only
  when they share files or a theme. The limits are an upper bound: under
  300 changed files, so an automated review covers the diff, and about
  3,000 changed lines. Write one `Closes #N` line for each issue the pull
  request completes; the issues stay as the specifications.
- **Closing keywords close.** GitHub reads `Closes #N` inside a negation
  too; "does not close #N" once closed the planning issue. Write a
  closing keyword only for an issue the pull request completes, and name
  every other issue without one.
- Branch from and target `main`; `release/1.0.0` is cut only to qualify
  release candidates. The planning issue is #3673 for 0.6.0, the 1.0
  interface released for users to test (D289), and #3950 for 1.0.0, which
  follows their feedback; #3030, closed, holds the design and the decisions
  D1 to D230, which stay the record and keep their citations.
- **Open ready for review**, not as a draft, once it builds and its nearest
  tests pass (D148). The one exception is a pull request that cannot merge
  until another one merges: it stays a draft, with `[hold: after #N]` at
  the start of its title.
- **Push once, validated.** Half of the pull request CI runs of October
  2026 were cancelled by a follow-up push. Run the local checks, then push;
  hold style and wording fixes for the next push that CI must run anyway.
- Before merging, bring the branch up to date with its base and wait for
  green CI on that exact head. A pull request that is green against an older
  base can still fail to compile against the current one.
- A decision recorded on a planning issue is not worked around. When one
  turns out to be wrong in practice, say so on the child issue with the
  evidence.
- Evidence such as run ids and artifact names goes in issues, never in
  committed files.

## Parallel sessions

Several agent sessions work the 1.0 plan at once, each on its own batch
of issues. These rules keep them from colliding; `CONTRIBUTING.md` owns the
merge steps and the review gate.

- **Claim by assignment.** Assign yourself to each issue before creating a
  branch, or link the pull request on the issue when you cannot assign.
  Leave an issue another session holds. A follow-up found in review goes to
  the session that owns the chain it belongs to.
- **One worktree per pull request, one Cargo target per worktree, one
  sccache cache per machine.** Create the branch with `git worktree add
  --no-track -b <branch> .worktrees/<name> origin/main` and push with an
  explicit refspec, `git push -u origin HEAD:refs/heads/<branch>`: a branch
  that tracks `origin/main` otherwise pushes to main. Give each worktree
  its own target, `CARGO_TARGET_DIR=<machine>/yamaa-target/<name>`, never
  a target another worktree uses: Cargo names a workspace crate's
  artifacts without the worktree's path, so two worktrees write the same
  file names into a shared target, and Cargo, which trusts modification
  times, compiles one worktree against the other's artifacts (D232).
  Share the compilation instead with sccache: install it once per machine
  (`cargo install sccache`) and give every worktree `RUSTC_WRAPPER=sccache`,
  one `SCCACHE_DIR=<machine>/yamaa-sccache` and a `SCCACHE_CACHE_SIZE`
  above the 10G default, such as 50G, so each locked dependency compiles
  once per machine and every other target takes it from the cache. The
  cache saves time, not disk: each target holds its own artifacts, so
  remove a worktree and its target when its pull request merges. sccache
  caches nothing compiled incrementally, which is only the workspace
  crates in the edit-build loop; set `CARGO_INCREMENTAL=0` for a one-off
  check (Clippy, the full suite).
- **Sequence shared files.** Read the issue's "Order" line and its
  sequencing comments before starting. An issue that edits a file an open
  pull request reshapes waits for that pull request.
- **Prove engine changes on the exact head.** CI's `python` and `r` lanes
  run equivalence on their fresh installations for engine/build changes.
  Both lanes must be green on the exact head, with their equivalence checks
  run. That is the proof for every session; no session needs to run the gate
  locally (D148 on #3030, with the owner-approved CI layout of #3658).
- **Get CI green on the exact head, then stop.** Once its pull request is
  open, the author gets CI green on its exact head, answers its reviews,
  and stops there. It neither reviews nor merges the pull request: the
  owner's maintenance session does both (D175 on #3030).

## Decisions

The owner's decisions are D-numbers: D1 to D230 on #3030, and D231 onward
on the planning issue #3673, which continues the sequence. Three kinds of
choice reach it; everything else is recorded where the work is.

- **Ask the owner** before building a new user-facing concept, reversing a
  D-decision, or changing a golden file no D-decision covers.
- **Record on #3673, as the next D-number**, a routine decision that
  changes what a user sees: a condition, context key, spelling, saved byte,
  printed text or host surface settled under the owner's standing
  instruction. One short paragraph: what, why, and the pull request. A
  session that cannot reach the counter writes "D-next" and the 1.0
  session numbers it.
- **Record in the pull request body** everything else: how an internal
  behaviour was built, which option an issue's question took, what a
  refactor left out. The pull request is the record; a comment on #3673 is
  not needed and the log stays readable. The 1.0 session folds a decision
  into the log only when a later issue cites it.

## Before a push

Run `make setup` once. Pull request CI selects the relevant `make check`
work and host equivalence in four lanes; engine/build changes run all of it.
Run only these locally (D148 on #3030):

- the tests nearest the change, listed below;
- after a Rust change, `cargo fmt --all --check` and Clippy on each
  touched crate, from `rust/`:
  `cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`,
  which also holds the 250-line function ceiling (`rust/clippy.toml`),
  and the 3,000-line file ceiling, from the root:
  `uv run --project python --no-sync python -m tools.validation.check_source_size`;
- `make quick`, only for a change to generated files, rule cards,
  schemas, docs generators or Python. It runs the lint and repository
  checks CI runs, leaving out only the R binding's Clippy, and no test
  suite, so it needs no R.

Never run `make check-equivalence`, or a host check with `--equivalence`,
locally, except to debug a CI failure of that gate. Native qualification
(`native.yml`) runs only in the regular CI/CD on main: weekly, and at the
candidate and release cuts (D233 on #3673). Never dispatch it on a pull
request's branch or push the branch as `qualification/issue-*`, never wait
for a native run before opening or finishing a pull request, and never
reproduce its jobs locally. Pull request CI runs only on Linux, so a pull
request that changes packaging, paths, file handling, `cfg`-gated code or a
workflow says so in its body: the next native run on main is its native
evidence, and a failure there becomes a child issue of the planning issue.
A "Done when" line that asks for a native run on the branch is met by that
run on main.

The tests nearest the change:

- **Rust** (`rust/`): from `rust/`, `cargo test --locked -p <crate>`
  for each crate touched. Portable-math controls live in
  `CONTRIBUTING.md`.
- **Python host or tools** (`python/`, `tools/`): the touched test
  files with `uv run --project python --no-sync python -m pytest <path>`.
  After changing `rust/` or `python/src/`, run `make setup` again
  first; the tests import the installed package.
- **R host** (`R/`): `make test-r` when R 4.6.1 is installed;
  otherwise CI's `r` job checks it.
- **Benchmark** (`benchmarks/`): follow `benchmarks/AGENTS.md`, then
  run the case from the root with
  `uv run --project python --no-sync python -m tools.benchmark benchmarks/<name>`.
- **Rules, schemas or docs** (`rules/`, `yaml/`, `docs/`, `README.md`):
  `make quick` checks them and builds the strict site; a docs-only
  change needs only `make docs-test docs`. Follow the terminology and
  requirement-allocation rules in `rules/README.md` and the maintenance
  rules in `yaml/README.md`.

When a CI job fails, reproduce it with that job's own target from
`make help` (`check-lint`, `check-repository`, `test-rust`,
`test-python` or `test-r`) rather than all of `make check`. Complete
native release qualification is a separate route described in
`CONTRIBUTING.md`.

## Writing tests

A test proves one authored expectation of one behaviour through the
surface its crate or package owns. `CONTRIBUTING.md#writing-tests` has
the patterns and helpers of each language; these are the rules.

- **Author or derive the expectation; never capture it.** An expected
  value is written by hand, read from authored truth
  (`rust/tests/host-fixtures/`, a crate's `tests/fixtures/`, or what a
  benchmark itself states), or derived from a rule card, the condition
  registry, a grammar file or the engine's documented geometry, with the
  derivation stated in the pull request. Never paste what the
  implementation printed, and never add a snapshot library.
- **Name the behaviour.** A test's name is the one sentence it proves,
  such as `case_otherwise_reports_inside_the_otherwise_member`, under
  the modules or classes that give it a path; a test a rule card cites
  as evidence keeps its name. A file holds one family, named in its
  first comment: a new family is a new `tests/<area>.rs` with its `mod`
  line in the crate's `tests/main.rs`, a new `test_<area>.py` or a new
  `<area>.R`. Installed Python suites use
  `python/tests/installed/installed_<area>.py` instead.
- **Use the owned helpers.** Build inputs with the crate's
  `tests/support/` factories, `tools/tests/rules_fixture.py` and the
  readers beside `rust/tests/host-fixtures/cases.json`. The second file
  that needs a helper moves it there; a helper stays in the crate or
  package whose tests use it.
- **A fixture before a test function.** A case that writes files or
  documents, makes one call and compares rows or bytes is a case in an
  owned fixture home (a benchmark, a `host-fixtures` family, a crate's
  `tests/fixtures/`), run by that home's reader
  ([Design principles](#design-principles)). A test function is written
  only for a seam the readers cannot reach, and says which in its first
  comment.
- **Assert the whole record, once, with a label.** Compare the complete
  observable value, such as a diagnostic's phase, condition, rule, paths,
  context and optional `source_span` and `operand_route` (including their
  absence), a table or saved bytes, with one authored literal, and
  name the case in the message, rather than chaining partial checks or
  `assertTrue(any(...))`.
- **Cases that differ in one value share one body.** Loop over a table
  of input, expectation and label in Rust, use `subTest(case=...)` in
  Python and a vector of cases in R. A separate test exists only when
  its setup differs.
- **Stay hermetic and cheap.** A scratch directory comes from the owned
  helper and is removed when the test ends. A test never sleeps, reads
  the network or depends on the environment beyond a declared skip with
  its reason, and spawns a program only when that program is its
  subject. A walk of the benchmark corpus or a build runs once per
  module, never once per test.
- **Keep each behaviour tested once.** A behaviour moved or rewritten
  takes its tests with it. A case both hosts check lives once, in the
  shared `host-fixtures` truth that each host's suite reads. Split a
  file by feature before it passes 3,000 lines (#2413), the ceiling of
  [Design principles](#design-principles).

## Pointers

- Subagent definitions for the 1.0 orchestration (an engineer, a release
  engineer, a writer and a reviewer, each deferring to this guide):
  `.claude/agents/`
- Language contracts and terminology: `rules/README.md`
- Schemas, grammars, and conformance fixtures: `yaml/README.md`
- Benchmark authoring: `benchmarks/AGENTS.md`
- Architecture, crate boundaries, and local verification: `CONTRIBUTING.md`
- Writing tests: `CONTRIBUTING.md#writing-tests`
- Design principles in practice: `CONTRIBUTING.md#design-principles-in-practice`
- Rust workspace entry point: `rust/README.md`
- Host usage: `docs/articles/api.md`
- Site build and canonical pages: `CONTRIBUTING.md#documentation`
- Benchmark factory: `automation/benchmark_factory/README.md`
- Harbor evaluation: `evaluations/harbor/README.md`
- Release, platforms, and migrations (not needed for normal changes): `docs/release.md`
