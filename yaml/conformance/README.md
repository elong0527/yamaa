# Conformance fixture identity

The [versioning policy](../../docs/versioning.md#versioned-conformance-suite)
defines the coordinated suite version, stable citations and update process.
This directory contains shared language vectors; it is one part of the suite,
not a second inventory or a record of passing runs.

The entire suite at release `0.4.0` is `yamaa-conformance/0.4.0`, pinned to the
exact commit of `yamaa-v0.4.0`. Before publication, cite the full development
commit. A stable vector reference includes that commit, this directory's file
path and the case `id`. The regex file's `contract_version` versions only its
portable regex contract; `schema_version` describes the fixture document.
Neither substitutes for the suite revision.

Other suite inputs and consumers live under `benchmarks/`, `yaml/grammar/`,
`rust/crates/*/tests/`, `rust/tests/`, `python/tests/installed/` and
`rust/qualification/`. Existing discovery and explicit conformance exclusions
define what each runner checks. Use the same revision's inputs, independently
authored truth, comparator and runner; retain the installed package/core
versions and `source_revision` with observations. Never generate expected data
from the candidate engine or retain passing-status ledgers here.
