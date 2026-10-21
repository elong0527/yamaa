# yamaa agent guide

This file is the single entry point for agents. It maps the repository,
states the hard rules, and says what to run before a push. Area guides (linked below) add detail; none of them overrides
this file. Release docs are not needed for normal changes: read
`docs/release.md` only for a release, a supported-platform change, or a
migration decision.

Run `make help` at the repository root for the supported checks and
their prerequisites. Local targets and CI share the owned scripts under
`tools/commands/`.

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
  reviews and merges every pull request (D175 on #3030). It merges only
  once CI is green on the exact head, by the merge steps in
  `CONTRIBUTING.md` (D176). On that head, require CodeRabbit's explicit
  completed review, or complete bounded reviews proved by the review gate,
  and resolve every actionable finding. Pending, missing, skipped alone or
  older-head reviews do not count. When environment policy blocks GitHub
  CLI reads, approved authenticated connector reads must prove the same
  predicates on fresh exact-head evidence. The author never reviews its own
  PR (D148), and a review that another agent records is welcome as a
  review record. When CodeRabbit cannot review a PR, `CONTRIBUTING.md`
  says what replaces its review.
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

## Pull requests

- One theme per pull request (D171 on #3030). Group related issues into
  one pull request of about 1,500 to 3,000 changed lines and under 300
  files, with one `Closes #N` line for each issue it completes; the issues
  stay as the specifications. Branch from and target `main`;
  `release/1.0.0` is cut only to qualify release candidates. The planning
  issue is #3030, for 1.0.0; #2848 tracks the 0.5.0 GitHub release.
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
of issues.
These rules keep them from colliding; `CONTRIBUTING.md` owns the merge
steps and the review gate.

- **Claim before you start.** Assign yourself to each issue of the
  batch, or comment "Claimed by <session>" when you cannot assign, before
  creating a branch, and leave an issue another session has claimed. A
  follow-up found in review goes to the session that owns the chain it
  belongs to.
- **One worktree per pull request, with its own Cargo target.** Create the
  branch with `git worktree add --no-track -b <branch> .worktrees/<name>
  origin/main` and push with an explicit refspec,
  `git push -u origin HEAD:refs/heads/<branch>`: a branch that tracks
  `origin/main` otherwise pushes to main. Never share `rust/target` or
  `CARGO_TARGET_DIR` between worktrees. Each target grows to 35 to 45 GB,
  so keep to about five Rust worktrees on one machine, and remove a
  worktree, or its target, when its pull request merges.
- **Closing keywords close.** GitHub reads `Closes #N` inside a negation
  too; "does not close #N" once closed the planning issue. Write a
  closing keyword only for an issue the pull request completes, and name
  every other issue without one.
- **Open pull requests ready for review.** A pull request opens ready for
  review, not as a draft, once it builds and its nearest tests pass (D148
  on #3030). The one exception is a pull request that cannot merge until
  another one merges: it stays a draft, with `[hold: after #N]` at the
  start of its title (owner decision of 2026-10-20, recorded on #3030).
- **Prove engine changes on the exact head.** CI's `equivalence` and
  `conversion` jobs, green on the exact head, are the proof for every
  session, and no session needs to run them locally (D148 on #3030).
  The `equivalence` job means its two legs, `equivalence (python)` and
  `equivalence (r)`, both green.
- **Get CI green on the exact head, then stop.** Once its pull request is
  open, the author gets CI green on its exact head and stops there. It
  neither reviews nor merges the pull request: the owner's maintenance
  session does both, as the hard rules say (D175 on #3030).
- **Decide under the owner's standing instruction, and record it.** A
  routine decision follows the recorded plan and is written on #3030 as
  the next D-number, so the owner can see and reverse it. D-numbers come
  from one shared counter, so parallel sessions never take the same
  number; a session that cannot reach the counter posts "D-next" on the
  planning issue for the 1.0 session to assign. Ask the owner only about
  a new user-facing concept, a reversal of a D-decision, or a golden
  change that no D-decision covers.
- **Sequence shared files.** Read the issue's "Order" line and its
  sequencing comments before starting. An issue that edits a file an open
  pull request reshapes waits for that pull request.

## Before a push

Run `make setup` once. Pull request CI runs the complete `make check` and
the `equivalence` and `conversion` jobs, so run only these locally (D148
on #3030):

- the tests nearest the change, listed below;
- after a Rust change, `cargo fmt --all --check` and Clippy on each
  touched crate, from `rust/`:
  `cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`;
- `make quick`, only for a change to generated files, rule cards,
  schemas, docs generators or Python. It runs the lint and repository
  checks CI runs, leaving out only the R binding's Clippy, and no test
  suite, so it needs no R.

Never run `make check-equivalence` or `make check-conversion` locally,
except to debug a CI failure of that job. Pull request CI runs only on
Linux, so a change to paths, file handling, `cfg`-gated code or
packaging also gets one native qualification run on its branch before
merge: `gh workflow run native.yml --ref <branch>`.

While developing, run only the tests nearest the change:

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

- **Author the expectation; never capture it.** An expected value is
  written by hand or read from authored truth: `rust/tests/host-fixtures/`,
  a crate's `tests/fixtures/`, or what a benchmark itself states. Never
  paste what the implementation printed, and never add a snapshot library.
- **Name the behaviour.** A test's name is the one sentence it proves,
  such as `case_otherwise_reports_inside_the_otherwise_member`, under
  the modules or classes that give it a path; a test a rule card cites
  as evidence keeps its name. A file holds one family, named in its
  first comment: a new family is a new `tests/<area>.rs` with its `mod`
  line in the crate's `tests/main.rs`, a new `test_<area>.py` or a new
  `<area>.R`.
- **Use the owned helpers.** Build inputs with the crate's
  `tests/support/` factories, `tools/tests/rules_fixture.py` and the
  readers beside `rust/tests/host-fixtures/cases.json`. The second file
  that needs a helper moves it there; a helper stays in the crate or
  package whose tests use it.
- **Assert the whole record, once, with a label.** Compare the complete
  observable value, such as a diagnostic's phase, condition, rule, paths
  and context, a table or saved bytes, with one authored literal, and
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
  file by feature before it passes about 3,000 lines (#2413).

## Pointers

- Language contracts and terminology: `rules/README.md`
- Schemas, grammars, and conformance fixtures: `yaml/README.md`
- Benchmark authoring: `benchmarks/AGENTS.md`
- Architecture, crate boundaries, and local verification: `CONTRIBUTING.md`
- Writing tests: `CONTRIBUTING.md#writing-tests`
- Rust workspace entry point: `rust/README.md`
- Host usage: `docs/articles/api.md`
- Site build and canonical pages: `CONTRIBUTING.md#documentation`
- Benchmark factory: `automation/benchmark_factory/README.md`
- Harbor evaluation: `evaluations/harbor/README.md`
- Release, platforms, and migrations (not needed for normal changes): `docs/release.md`
