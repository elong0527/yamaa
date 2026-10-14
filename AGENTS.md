# Conventions

## Find the current commands and contracts

Run `make help` at the repository root for supported checks and their prerequisites.
`make check-fast` provides repository feedback; installed-host checks and complete
native release qualification are separate routes. Local targets and CI share the
owned scripts under `.github/scripts/commands/` and the canonical `rust/tools/`.
Current architecture/development and release gates live in `docs/development.md`
and `docs/release.md`; language contracts remain in `rules/` and `yaml/`.

## The project name is written `yamaa`, never `YAMAA`

Lower case, always, including at the start of a sentence, in headings, in page
titles, in nav labels, and in doc comments. The hex logo spells it that way and
the site is built to match. Do not "fix" it to title case or upper case.

One place keeps capitals on purpose, because the string there is not the
project name being written as prose:

- **`YAMAA-01` in `benchmarks/**/input/` and `benchmarks/**/expected/`** is a
  CDISC `STUDYID` data value, and uppercase study identifiers are the
  convention in clinical data. It also lives inside committed `.parquet`
  fixtures, so changing it means regenerating binary test data.

## The two halves of the site share one palette

`docs/stylesheets/extra.css` styles the Material pages and
`.github/scripts/benchmark-docs/dashboard.css` styles the self-contained
benchmark dashboards. They are separate files on purpose -- a dashboard opens
from disk with no network request -- but they are meant to render as one site.
Change a color or a bar treatment in one and change it in the other.

## No content hashing

The repository does not hash content. A runtime artifact is pinned by
`runtime.artifact.reference` alone, a contract fingerprint is its canonical
RFC 8785 JSON rather than a hash of it, the activation cache key is the
joined identities themselves, and a resource snapshot compares the bytes it
already holds.

Do not reintroduce SHA-256 (or any digest field) to give something an
identity. Compare the canonical form directly, and keep the bytes when
equality has to be decided on bytes.

The packaging-tool exceptions are `python/uv.lock` and the canonical
`rust/Cargo.lock`, including unchanged copies staged into Python/R source
packages. Their checksum fields belong to the packaging tools and are not
the repository's own provenance. This exception does not permit runtime
identity digests, hashed contract fingerprints, activation-cache digests or
resource-snapshot digests.

## Label the benchmark runtime `opensas`

Use `opensas` in benchmark labels, prose, prompts, commands, dataset names,
leaderboards, and solution comments. Keep the standard `.sas` source extension,
upstream asset names, and required legal trademark attribution intact.

## Site pages have one source

Edit `README.md` for the site home page and `rules/README.md` for the rules
index. MkDocs reads those files through `.github/scripts/docs/canonical_pages.py`;
do not create `docs/index.md` or `docs/articles/rules.md`. Their site URLs and
Edit buttons are set by the hook. Relative Markdown links resolve from the
canonical source file. Build and test commands live in
`.github/scripts/docs/README.md`.

## Rust and host APIs

`docs/development.md` describes the crate boundaries and local verification
commands; `rust/README.md` is the Rust entry point. `docs/release.md` owns current
API/math/platform/rollback decisions and pending migration gates.
Language contracts live in `rules/`, and host usage lives in `python/README.md`
and `R/yamaa/README.md`. The declared API inventory is
`rust/qualification/release-api.json`; check it from the repository root with
`uv run --project python --no-sync python rust/tools/check_release_api.py`.

Current REQ owners and generated references derive from `rules/` and `yaml/`.
The independent schema-owner fixture is
`.github/scripts/yaml-validation/fixtures/schema-navigation.tsv`; preserve its
363 expectations when changing checkers. Run the rule/reference commands in
`docs/development.md` with the repository Python environment.

Portable math golden values live in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv` and are shared by the
Rust and Python tests. Run
`cargo test --locked -p yamaa-core --test portable_math_golden` from `rust/`.
Public math conversion, verification and publication checks run from the
repository root with
`uv run --project python --no-sync python -m unittest discover -s rust/tests -p test_portable_math.py`.
