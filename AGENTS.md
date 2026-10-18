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

- **Breaking a user-facing interface is a decision.** Until 1.0.0, a change
  that breaks the specification language, the Python or R API, saved outputs
  or diagnostic identity follows a decision recorded on #3030 or an owner
  decision on its own issue. It gets a one-line entry in the next release's
  notes under `docs/releases/` and no step-by-step migration text. Internal
  interfaces, including the unpublished Rust crates, may change freely.
  Remove obsolete interfaces and callers together instead of adding
  compatibility aliases. From 1.0.0, the stability contract of #3030
  section 2.11 applies.
- **Complete review before merging.** On the exact final PR head, require
  CodeRabbit's explicit completed review, or complete bounded reviews proved
  by the review gate, and resolve every actionable finding. Pending, missing,
  skipped alone or older-head reviews do not count. Run the fresh read-only
  review gate in `CONTRIBUTING.md` before every merge, alongside CI. When
  environment policy blocks GitHub CLI reads, approved authenticated connector
  reads must prove the same predicates on fresh exact-head evidence. When
  CodeRabbit cannot review a PR (its free reviews are exhausted, or it skips
  the PR), merging does not wait for it: the merging agent reviews the exact
  diff itself, records that review on the PR, and still requires green CI on
  that exact head.
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
  exceptions are `python/uv.lock` and `rust/Cargo.lock`, including
  unchanged copies staged into Python/R source packages; their checksum
  fields belong to the packaging tools.
- **Site pages have one source.** Edit `README.md` for the site home
  page and `rules/README.md` for the rules index. MkDocs reads those
  files through `tools/docs/canonical_pages.py`; do not create
  `docs/index.md` or `docs/articles/rules.md`.

## Pull requests

- One issue per pull request, referenced with `Closes #N`. Branch from and
  target `main`; `release/1.0.0` is cut only to qualify release candidates.
  The planning issues are #3050 for the current milestone and #3030 for 1.0.0.
- Before merging, bring the branch up to date with its base and wait for
  green CI on that exact head. A pull request that is green against an older
  base can still fail to compile against the current one.
- A decision recorded on a planning issue is not worked around. When one
  turns out to be wrong in practice, say so on the child issue with the
  evidence.
- Evidence such as run ids and artifact names goes in issues, never in
  committed files.

## Before a push

Run `make setup` once, then `make quick` before each push. It runs the
lint and repository checks CI runs, leaving out only the R binding's
Clippy, and no test suite, so it needs no R. Pull request CI runs the
complete `make check`, so there is no need to run it locally.

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
  `make quick` checks them and builds the strict site. Follow the
  terminology and requirement-allocation rules in `rules/README.md` and
  the maintenance rules in `yaml/README.md`.

When a CI job fails, reproduce it with that job's own target from
`make help` (`check-lint`, `check-repository`, `test-rust`,
`test-python` or `test-r`) rather than all of `make check`. Complete
native release qualification is a separate route described in
`CONTRIBUTING.md`.

## Pointers

- Language contracts and terminology: `rules/README.md`
- Schemas, grammars, and conformance fixtures: `yaml/README.md`
- Benchmark authoring: `benchmarks/AGENTS.md`
- Architecture, crate boundaries, and local verification: `CONTRIBUTING.md`
- Rust workspace entry point: `rust/README.md`
- Host usage: `docs/articles/api.md`
- Site build and canonical pages: `CONTRIBUTING.md#documentation`
- Benchmark factory: `automation/benchmark_factory/README.md`
- Harbor evaluation: `evaluations/harbor/README.md`
- Release, platforms, and migrations (not needed for normal changes): `docs/release.md`
