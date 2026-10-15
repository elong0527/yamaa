# Contributing to yamaa

This is the one contributor guide. `AGENTS.md` is the entry point and
states the hard rules; area guides (`rules/README.md`, `yaml/README.md`,
`benchmarks/AGENTS.md`, `rust/README.md`) own their contracts. Release
platforms, approved decisions, implementation limits and migrations live
in `docs/release.md` — read it only for a release, a supported-platform
change, or a migration decision.

yamaa uses one shared Rust engine for the Python and R packages. Rules
define behavior; independently authored expected files under
`benchmarks/**/expected/` define executable test truth. Never regenerate
expected files from the implementation, and never edit
`benchmarks/**/input/` or `benchmarks/**/expected/` to make a failing
implementation pass.

## Where changes belong

| Area | Source and responsibility |
| --- | --- |
| Language | `rules/`, `yaml/`: normative contracts, diagnostic registry and schemas. |
| Shared semantics | `rust/crates/yamaa-core/`: values, expressions, admission and compilation. |
| Shared lifecycle | `rust/crates/yamaa-engine/`: execution, resource/function ports, verification and publication. |
| Storage and formats | `rust/crates/yamaa-adapters/`: YAML, CSV, Parquet, files, project environments and submission rendering. |
| Host boundaries | `rust/crates/yamaa-python/`, `rust/crates/yamaa-r/`: arguments, callbacks, scalar conversions and owned result views. |
| Host packages | `python/`, `R/yamaa/`: thin public facades, private style/ODM helpers and package builds. |
| Installed qualification | `rust/tests/`, `R/yamaa/tests/`, `rust/qualification/`: installed contracts, independent fixtures and required coverage. |
| Automation | `.github/workflows/`, `.github/scripts/`, `rust/tools/`: CI selection, staging, comparisons and evidence replay. |

Core and engine remain `no_std` and exclude host/storage frameworks.
Diagnostics belong to the shared registry. Host callbacks carry typed
values; diagnostic JSON text does not introduce a callback request/reply
protocol. Build computes and retains results; explicit Save publishes
held bytes in the contract's order. Resource-refusal diagnostics and
implementation limits are specified once in `docs/release.md`.

## Commands

Run `make help` at the repository root for the supported targets, their
prerequisites and configuration variables; this table is a summary, not
a second inventory. The root `Makefile` calls the same owned scripts
under `.github/scripts/commands/` as CI. GNU make and Python 3.12 or
later are enough to read the help inventory; individual checks need the
tools listed there. Use Rust 1.90.0 and the canonical `rust/Cargo.lock`.

| Target | Checks and context |
| --- | --- |
| `check-fast` | Python formatting/lint and CI selector/command controls in the synced development environment. |
| `check-repository` | Validator controls, available-host repository validation, style, inventory, documentation and generated rule/reference guards. |
| `check-blockers` | Live validation-blocker issues; requires GitHub API access and its token. |
| `test-rust` | Locked shared-workspace debug tests, engine `test-support`, then optimized `release-test` tests. |
| `stage-python`, `test-python` | Stage the cataloged supplemental suites and run them through a separately installed package outside the checkout. |
| `stage-r`, `stage-r-tests`, `test-r`, `check-r` | Stage the standalone R source and cataloged suites; run installed checks or a fresh package check requiring **Status: OK**. |
| `docs-test`, `docs-generate`, `docs-build`, `docs` | Canonical-page tests, dashboard generation and strict site build with the pinned documentation dependencies. |

Start with `make sync-python` (uv 0.11.6; installs the locked
development test environment), then the recipe in `AGENTS.md` for the
area you touched. Portable math has shared authored golden values in
`rust/crates/yamaa-core/tests/fixtures/portable_math.tsv`; run
`cargo test --locked -p yamaa-core --test portable_math_golden` from
`rust/`. Paired installed Python/R inputs and expected values have one
shared owner in `rust/tests/host-fixtures/cases.json`; its README maps
the paired families. Installed-host commands, staging variables and
comparison rules are in `rust/qualification/README.md`.

## Repository validation

Repository validation checks authored specifications, schemas, rules,
fixtures, style and documentation. The validator uses the installed
shared Rust engine through public `yamaa.check` and bounded native
project metadata services; repository schema/layout/editorial checks
remain in Python. `validate_repository.py` under
`.github/scripts/yaml-validation/` orchestrates the focused check
owners and presents their findings; there is no second language
evaluator. Run it through the recipes in `AGENTS.md`
(`make check-repository`, or the `repository.py` commands named there).

Automatic PR feedback uses the lightweight repository checks in
`.github/workflows/pr.yml`; complete semantic validation and
deterministic dashboard generation run on main pushes or manual
`yaml-validation.yml` dispatches. The validator does not execute
clinical derivations or prove golden output values: full installed
domain/check/Define, R grammar and cross-platform parity evidence
belongs to the native workflow, not a repository-validation pass.

## CI overview

Keep workflow definitions in `.github/workflows/` and supporting code in
`.github/scripts/`, grouped by purpose (`benchmark-docs/`,
`yaml-validation/`, `ci/`, `docs/`, `commands/` — the last is shared by
the Makefile and CI). Pin external actions to full commit hashes with
exact release comments.

- `pr.yml` is the only automatic PR workflow: three Linux jobs
  (`repository-validation`, `pr-rust-smoke`, `pr-python-smoke`) run the
  scoped smoke route. Runtime jobs inspect the merge-base diff and skip
  build/test steps on prose-only changes, but still report success, so
  required checks always appear. PR feedback does not establish
  installed or release qualification.
- `unit-tests.yml` runs on main pushes or manual dispatch: all shared
  Rust, Python package, helper and installed R tests, plus the combined
  coverage badge the docs workflow deploys to `/coverage/`.
- `native.yml` retains the full three-OS installed matrix, shared Rust
  checks and the public/producer cohort gate: nightly on main
  (05:17 UTC), by manual dispatch, or on `qualification/issue-*`
  branch pushes. For pre-merge qualification, dispatch it on the exact
  branch head and require every enabled lane to pass.
- `yaml-validation.yml` runs complete repository semantic validation
  and live validation-blocker checks on main pushes or dispatch, and
  deterministic dashboard generation when a dashboard source changed.
- `harbor-evaluation.yml` / `harbor-oracle.yml` run full reference
  grading and Oracle verification on relevant main pushes, schedules,
  or manual dispatch; PR smoke runs only lightweight Harbor tests.

## Documentation build

Edit the root `README.md` for the site home page and `rules/README.md`
for the rules index; the MkDocs hook in
`.github/scripts/docs/canonical_pages.py` renders those canonical
sources as virtual pages — there are no copies to synchronize. The
requirement index and schema-fields pages are rendered by the same hook
from `rules/` and `yaml/` on every build and are not committed. Other
site pages live in `docs/`.

Install the pinned dependencies into the selected interpreter, then use
the Make targets (`DOCS_PYTHON` selects another interpreter):

```sh
"${DOCS_PYTHON:-python}" -m pip install -r docs/requirements.txt -r .github/scripts/benchmark-docs/requirements.txt
make docs-test
make docs
```

`docs-build` requires generated dashboards; `docs` generates them
before building. The conference slide decks under `docs/slides/` are
kept in the repository as historical material and are not part of the
deployed site build.

## Reporting findings from private studies

Everything in this repository is public, including edit history. A
finding from a confidential study may reach GitHub only as public
references plus an example rebuilt from scratch with invented data:

- Never post participant data (even with identifiers replaced), study
  identity (sponsor, protocol, real `STUDYID`, sites), unpublished
  specifications or results, internal paths/hostnames/user names, or
  secrets. Paste text, never screenshots or binary attachments.
- Anchor the report on public references (rule file, REQ ID, `yaml/`
  entry, benchmark), rebuild a minimal example from the language —
  never by trimming a real spec or records — and confirm it reproduces
  on the latest `main`.
- The reviewer, not the agent, approves the exact final text before
  anything is posted; follow-ups repeat the same check. If the evidence
  cannot be made public, keep it inside your organization's process and
  file at most a general public issue without it. If something
  confidential was posted, stop, edit it out (including edit history),
  and rotate any exposed credential.

The SDTM-to-ADaM derivation recipe for pilot teams is
`docs/articles/sdtm-to-adam.md`.
