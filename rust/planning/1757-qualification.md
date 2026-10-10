# Public environment qualification for #1757

The closing change is based on main
`ee8b943b0fc88fddae9950fb58c25ea91bdfa75c`. It implements the approved
[maintainer decisions](1757-maintainer-decisions.md). Local results below are
qualification of the changed source on macOS arm64 with Rust 1.90.0, Python
3.14.0 and R 4.6.1. They are not evidence for unexecuted platform tuples; the PR
must pass the installed CI matrix before merge.

## Acceptance ownership

| #1757 gate | Closing implementation and witness |
| --- | --- |
| Rules, schemas and conventions | Authoritative environment/function/codelist/submission classes; versionless calls; generated shipped schema and rule references; independent admission and repository validation tests. |
| Static `check` without code or data | Shared preparation and bounded complete static issue projection in both public hosts. Installed tests remove the study input and forbid activation effects while checking calls, lock format, language, function coverage and codelist bindings. |
| Per-build lock and tests before study reads | Ordinary installed host capabilities feed the shared owned project lifecycle. Repeated builds retain every attempted binding/test and prove lock or test failures precede all study capture. No activation cache remains. |
| Environment terminology | Existing independent catalogue, binding, fixed-set equality and runtime REQ-0957 tests remain; installed public tests admit and execute inline sources and collect unknown-list and invalid-call findings together. |
| Equivalent Python/R environments | The shared Rust pipeline drives both installed public suites, including output, issues, activation evidence and explicit saved bytes. Python additionally retains original interrupts and opaque host failures. |
| Five function benchmarks | Equivalent Python/R definitions use normally installed `yamaa-benchmarks`/`yamaabenchmarks` packages and existing UV or hash-free renv locks. Four exact original CSV files pass twice in each host. The negative study fails statically in both hosts with the independently specified replacement below. |
| Superseded modules and formats | Artifact resolution, fingerprints, cached activation, contract/conformance loaders, vendored runtime code and their obsolete consumers are retired. Installed component protocol truth remains where it tests the shared implementation. |

## Intentional compatibility changes

`negative-function-contract/expected/error.yaml` is the sole changed expected
artifact. The removed requested-contract-version mismatch becomes
`invalid_function_argument`, REQ-0700, at
`columns.RESULT.derivation.function.args`, with independent context identifying
missing required argument `x` for `project_value`. No positive expected artifact
is regenerated or loosened.

The shared specification compiler selects pinned `libm` 0.2.16, default features
disabled, under internal `PortableLibmV1` for POWER/EXP/LN. Public Python/R tests
cover column, row and nested case compute with independent numeric and exact CSV
truth. No API or YAML policy selector is exposed. Historical platform-math
comparison remains intact: the local 30,033-case assessment records 989 EXP,
279 LN and 950 POWER differences from historical Python, each at most one ULP
in that sample. Its exact-parity status remains `blocked-by-mismatches`.
The approved disposition accepts those differences and their downstream effects;
it does not imply universal correct rounding or close full-release math gates.

Three obsolete function-specific `reference_assisted_run` required rows move to
the installed shared public suite: `schema-functions`, `schema-non-finite` and
`negative-function-contract`. The reference component runner explicitly refuses
project calls after retirement; it cannot manufacture shared execution evidence.
The complete benchmark inventory still shows unsupported/missing routes. All
other required rows and independent component truth stay in place. Supplemental
inventories retain 24 Python suites and 18 R suites, with the new public suites
classified as `shared_run` separately from component/compile contracts.

The execution manifest records `reference_retired_by: '#1757'` for the five
project studies. Their installed public suites replace the retired reference
baseline gate, while all other executable reference baselines remain required.
Retirement does not turn a missing or unsupported report into a pass, suppress
a supplied failing report, or allow native parity qualification without its
reference. The broader inventory retains every route and its actual result.

## Qualification and remaining scope

Local qualification passes 987 shared Rust tests in each of debug and optimized
`release-test`, strict workspace Clippy, 48 Rust tooling tests, 411 repository
validator tests, dependency/release-API guards, rule/style checks and Python
formatting/lint. Both installed Python forms pass all 24 supplemental suites;
the updated public suite passes 13 methods directly in each form. The R source
archive passes all 18 supplemental suites and strict `R CMD check` with
`Status: OK`. The full Python regression replay passes 4,814 tests with 94
skips. The retained legacy R submission schema copies remain byte-identical to
the canonical shared/metadata/Define schemas.
CI independently rebuilds packages from the PR revision and exercises all six
Python OS/package-form lanes and both exercised R platforms. The existing
Windows R gap remains explicit under #1742.

No new digest-bearing lock or identity is introduced. The Python benchmark group
reuses `python/uv.lock`; R locks record package versions without hashes. The
existing resource-root configuration remains until its own path-policy decision.

The old Define-XML schema, generator and three submission fixtures remain until
#1758 qualifies. Environment admission carries submission sections and
`submission.has_no_data`; it does not replace submission output. Shared producer
qualification remains #1741. Full supported-language/API coverage, reproducibility,
representative performance, release transition and remaining numerical/platform
gates remain #1751/#1752/#1754/#1740/#1742/#1585. Closing #1757 does not close the
Rust migration as a whole.

The final review corrections preserve nested numeric/predicate grammar paths,
reject whole-column windows inside cases and unimplemented implicit-group source
predicates before study authority, allow uncalled uv records without resolved
versions, retain pathless repository rejections, and validate R capabilities
before metadata capture. Duplicate retired project frontend checks are removed;
the actual installed public environment suites retain activation/data ordering
coverage. Focused regressions, both rebuilt Python forms and strict R source
checking cover these corrections. The Windows R release-scope decision is
tracked separately in [#1857](https://github.com/elong0527/yamaa/issues/1857).

Windows CI identified that public `environment=` absolutization produced native
backslashes while resource requests require portable separators. Caller-owned
absolute paths now use the existing Windows drive/UNC conversion; authored YAML
paths retain strict validation. A shared regression admits an absolute metadata
path with no study source present and counts metadata-only captures. The existing
installed absolute-path and five-study public tests exercise the real host API.
