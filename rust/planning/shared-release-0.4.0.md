# Shared release 0.4.0 preparation

This is a review-only version, notes and distribution slice on the isolated
removal candidate. Landing and publication require the combined-main installed
matrix, B17 budgets and the explicit item 11 transition decision in
[transition.md](transition.md). External R retirement on main 81a210ca is
preserved; it does not establish those gates. This slice composes complete native
validator prerequisite a0f6107d without relabeling its pending hosted evidence.

## Version and package coherence

Python distribution metadata, R DESCRIPTION and the Cargo workspace advance to
0.4.0 together. All five workspace member lock entries advance to 0.4.0, while
their manifests continue inheriting the workspace version. Cargo's workspace
update changes no third-party package resolution. The regenerated uv lock
changes the root project version and its packaging format revision; all
third-party package records remain identical. The five R benchmark locks pin
yamaa 0.4.0 and retain the independent function package at 1.0.0.

`check_shared_release_versions.py` requires coherent Python/R/workspace/member
versions, every Cargo member's exact locked identity and one owned yamaa project
in uv.lock. Installed public contract checks require package and embedded core
0.4.0; mandatory real-R validator transport keeps exact package/core identities
and its function package 1.0.0. Version outputs are compared directly.

The complete report contract identifier `0.3.0-draft` remains its existing
protocol version. Historical audit/replay records and retained prior packages
keep their original versions. Synthetic caller-version controls remain explicit
caller values rather than claims about the installed candidate.

## Define truth and defaults

The four authored Define environments already declare
`source_system_version: '1.0'`. Their fixed XML and dataset-JSON goldens therefore
remain byte-identical. Typed shared-Define tests pass an explicit caller version,
and core source-version admission tests preserve their authored 0.3.0/1.0/custom
input values. No expected file is regenerated or version-normalized.

Installed Python default controls remove the explicit field and require
`sourceSystem.version` and XML `SourceSystemVersion` to equal installed
`yamaa.__version__`. R defaults use the embedded core version; the coordinated
release's package/core equality is explicitly checked before public operations.
Those default controls consequently require 0.4.0 while the explicit-version
goldens retain 1.0. This preparation needs renewed installed Define qualification
in both hosts after the actual versioned artifacts are built.

## Proposed release notes

yamaa 0.4.0 uses one shared Rust engine for Python and R domain building,
declaration checks and Define generation. Python exposes domain/check/define,
owned result and error types, version metadata and read_odm/write_odm_parquet.
R exposes yamaa_domain/yamaa_check/yamaa_define. Retained results hold their
computed output; Save publishes held artifacts after source files disappear.
Verification and warning logs, diagnostics and full-range i64/NUL carriers retain
their qualified contracts.

The intentionally changed Python surface removes legacy yamaa_domain/DomainRun,
planner/model/evaluator interfaces, exposed resolved/input views, arbitrary Save
destinations and the old submission authoring API. The
[migration guide](../../python/README.md) lists replacement calls. Paths belong
to their declaring files; successful earlier producer publications remain if a
later target fails. Users update a fresh project lock and start a new whole run.

PortableLibmV1 fixes the shared EXP/LN/POWER implementation. Result bits,
downstream predicates, conversions and saved bytes can differ from historical
platform math. The [numerical decision](1740-math-decision.md) records that
compatibility change; this release promises neither historical bit identity nor
universal correct rounding.

## Distribution and pending proof

The required installed matrix is Linux/macOS/Windows, CPython 3.12/3.13/3.14,
wheel and independently rebuilt source forms, plus R 4.6.1 source installation on
all three platforms with strict R CMD check. Python produces the qualified wheel
and `yamaa-0.4.0.tar.gz`; R produces `yamaa_0.4.0.tar.gz` as the approved first
GitHub source asset. Both sources carry the canonical workspace, Cargo lock and
toolchain. R source installation requires Rust 1.90.0/Cargo and compiler/make
tools; Windows additionally requires Rtools45 and the GNU Rust target.

Final source revision, actual qualified artifact names, publication tag and
artifact URLs remain review fields until the versioned combined candidate passes.
Current build templates use the new filenames without publishing assets or
changing the existing rollback drafts. The actual post-removal/versioned revision
must renew membership/locked-build checks, all public/ODM/style/validator suites,
the complete 316-case ratchet, numerical evidence, performance budgets and audit.
