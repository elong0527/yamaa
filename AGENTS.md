# yamaa agent guide

This file is the single entry point for agents. It maps the repository,
states the hard rules, and gives the command that proves each kind of
change. Area guides (linked below) add detail; none of them overrides
this file. Release docs are not needed for normal changes: read
`docs/release.md` only for a release, a supported-platform change, or a
migration decision.

Run `make help` at the repository root for the supported checks and
their prerequisites. Local targets and CI share the owned scripts under
`.github/scripts/commands/`.

## Repo map

- `.github/` -- CI workflows and the owned check, docs, and validation scripts they run.
- `automation/` -- the benchmark factory and other maintainer automation, with its own guides.
- `benchmarks/` -- one directory per benchmark: spec, inputs, and authored expected output; see `benchmarks/AGENTS.md`.
- `docs/` -- the site pages other than the two canonical sources, plus development and release guides.
- `evaluations/` -- the Harbor agent evaluation: prompts, solutions, grading, and leaderboards.
- `python/` -- the Python host package: a thin facade over the shared Rust engine.
- `R/` -- the R host package under `R/yamaa/`, likewise a facade over the shared engine.
- `rules/` -- the normative language contracts, indexed by `rules/README.md`.
- `rust/` -- the shared Rust engine workspace, its tools, and installed-host qualification.
- `yaml/` -- the specification schemas, closed grammars, and conformance fixtures; see `yaml/README.md`.

## Hard rules

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
  files through `.github/scripts/docs/canonical_pages.py`; do not create
  `docs/index.md` or `docs/articles/rules.md`.

## Change recipes

Each recipe names the command that proves the change. Run the recipe
for every area a change touches.

- **Rule or schema change** (`rules/`, `yaml/`): follow the terminology
  and requirement-allocation rules in `rules/README.md` and the
  maintenance rules in `yaml/README.md`, then run
  `python .github/scripts/commands/repository.py rules` and
  `python .github/scripts/commands/repository.py rule-references`.
- **Rust change** (`rust/`): from `rust/`, run
  `cargo test --locked -p <crate>` for each crate touched, then
  `cargo clippy --locked --all-targets -- -D warnings` and
  `cargo fmt --all --check`. The portable-math golden command lives in
  `docs/development.md`.
- **Python host change** (`python/`): run
  `uv run --project python --no-sync pytest python/tests`.
- **New or changed benchmark** (`benchmarks/`): follow
  `benchmarks/AGENTS.md`. Run one positive benchmark from its own
  directory -- after `make sync-python` once, `cd benchmarks/<name>` and
  `uv run --project ../../python --no-sync python run.py` -- then run
  the repository validator from the root, which also checks negative
  benchmarks:
  `python .github/scripts/commands/repository.py validator`.
- **Docs change** (`docs/`, `README.md`, `rules/README.md`): run
  `make docs-test docs`.
- **Anything else, or a cross-area change:** `make check-fast` gives
  repository feedback; `make check-repository` runs the full repository
  checks. Installed-host checks and complete native release
  qualification are separate routes described in `docs/development.md`.

## Pointers

- Language contracts and terminology: `rules/README.md`
- Schemas, grammars, and conformance fixtures: `yaml/README.md`
- Benchmark authoring: `benchmarks/AGENTS.md`
- Architecture, crate boundaries, and local verification: `docs/development.md`
- Rust workspace entry point: `rust/README.md`
- Host usage: `python/README.md` and `R/yamaa/README.md`
- Site build and canonical pages: `.github/scripts/docs/README.md`
- Benchmark factory: `automation/benchmark_factory/README.md`
- Harbor evaluation: `evaluations/harbor/README.md`
- Release, platforms, and migrations (not needed for normal changes): `docs/release.md`
