# yamaa Rust migration release readiness

Assessed 2026-10-11 against main
`c8076646f403e2c15337c7771fa4341d2e5eaeee`. This is the finite blocker register
for [#1969](https://github.com/elong0527/yamaa/issues/1969). Earlier release and
prototype observations are preserved verbatim in
[the archive](archive/release-readiness-before-work-packages.md); their counts
and tested revisions remain historical, not final release evidence.

The approved matrix is CPython 3.12, 3.13 and 3.14 on Linux x86_64, macOS arm64
and Windows x64, each as a wheel and independent source rebuild (eighteen lanes),
plus R 4.6.1 source build/install and strict `R CMD check` on all three hosts.
Every tuple is required. The numerical policy, full i64, JSON-text diagnostic
context, declaring-file paths and Cargo packaging-checksum exception are approved
in [the maintainer decisions](maintainer-decisions.md). Approval sets the target;
it does not turn a missing tuple into a pass.

## Finite blocker register

Each open row identifies one primary work-package owner. Its related tasks and
dependencies remain in #1969. A row closes with the complete linked evidence or
an explicit approved disposition, not with a component-test count.

| ID | Current status and remaining evidence | Primary owner |
| --- | --- | --- |
| B01 | Bounded public cohorts exist. Complete public Python/R conformance against every supported benchmark and the full-matrix ratchet are open. | [#1974](https://github.com/elong0527/yamaa/issues/1974) |
| B02 | Public domain/check facades run on the shared engine. Installed signatures, argument errors, complete issues/log/output/save behavior and consumer migration remain. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B03 | Full-range R int64 and JSON-text context are approved. Lossless NUL-vector and retained-helper operations, deliberate removals and final installed surface qualification remain. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B04 | Declaring-file local paths are merged under #1905. The repository validator still enforces retired root containment and must align without weakening kind/link/snapshot checks. | [#1995](https://github.com/elong0527/yamaa/issues/1995) |
| B05 | Combined packages exist. Probe consumers must migrate before obsolete component endpoints and JSON transports are removed. | [#1990](https://github.com/elong0527/yamaa/issues/1990) |
| B06 | #1757 closed environment/function/codelist implementation. One complete engine use case must retain activation-before-data and all host effects in both bindings. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B07 | #1741 closed producer workflow implementation. Qualify its final composition with the unified use case, every port and failure/publication order. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B08 | #1758 closed shared Define/Dataset-JSON generation. Engine-level Define orchestration, submission metadata in domain builds and diagnostic ownership remain. | [#1986](https://github.com/elong0527/yamaa/issues/1986) |
| B09 | Shared language coverage remains incomplete. Mapping/cut is one owner in the broader #1975-#1983 language wave, plus #1986 and #2005; all require public ratchet acceptance. | [#1975](https://github.com/elong0527/yamaa/issues/1975) |
| B10 | File/codec/publication implementations are merged. Final unified port composition must retain exact CSV, logical Parquet, original failures and held-save behavior. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B11 | Portable registry exists, but adapter/core/engine diagnostic ownership, literal guards and normative null dispositions are incomplete. Two historical predicate gaps now match; ESCAPE policy implementation is #1983. | [#1984](https://github.com/elong0527/yamaa/issues/1984) |
| B12 | #1740 closed the PortableLibmV1 numerical decision. Every affected supported language scope still needs exact public truth on the complete installed matrix. | [#1974](https://github.com/elong0527/yamaa/issues/1974) |
| B13 | Canonical Cargo locking, unchanged packaged workspace membership including build scripts and locked builds are merged. Retain final clean transitive-feature/toolchain evidence in every approved package tuple. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B14 | Policy resolved: only packaging-tool checksum fields in python/uv.lock, rust/Cargo.lock and unchanged staged copies are allowed. Runtime/contract/cache/resource identity digests remain prohibited. Build evidence belongs to B13. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B15 | Complete support matrix is approved. #2000 is merged and its dispatched Windows R lane passes; full PR-head and combined-main runs are still incomplete at this assessment. Final all-lane release qualification remains open. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B16 | Boundary probes cover ownership/interruption/reentrancy/repeated use. Repeat those contracts through the final installed public facade, including failures and expired/mismatched handles. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B17 | Representative lookup, aggregate, window, callback and producer measurements at 1k/10k/100k rows are approved. PR #2003 is an open collector candidate; complete truth, comparable baselines, raw cold/warm samples, phase attribution and memory evidence remain. | [#1998](https://github.com/elong0527/yamaa/issues/1998) |
| B18 | Select numeric budgets after actual B17 measurements. Capacity refusals and unavailable metrics are not accepted timing/memory samples. | [#1973](https://github.com/elong0527/yamaa/issues/1973) |
| B19 | Write the concrete transition, release/rollback and evaluator-removal plan. Approve only after all entry criteria pass; keep the reference until then. Removal implementation is #1994. | [#1999](https://github.com/elong0527/yamaa/issues/1999) |

## API and final audit

[release-api.json](release-api.json) inventories declared exports and their
dispositions; `python rust/tools/check_release_api.py` rejects source drift.
#1991 must update its pending classifications to the approved dispositions and
qualify attributes, signatures, result operations and retained helpers outside
the checkout. The inventory guard does not prove those behaviors.

#1997 must audit the exact source revision/parents, every required lane, package
bytes, runtime/image/toolchain versions, numerical comparison and public/producer
reports. Missing, failed and unexercised tuples stay explicit. PR sampling is
feedback; full dispatched and main evidence supply qualification.

The historical assisted inventory and per-slice qualification ledgers are frozen.
Retire the assisted inventory only after #1974's full-matrix ratchet passes and
before deleting its probe consumers. Independent expected outputs remain fixed.
New evidence goes in the implementing PR and automated audit record; #1972 or
its wave-refresh successor owns this planning narrative.
