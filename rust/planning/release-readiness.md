# yamaa Rust migration release readiness

Assessed 2026-10-13 against main
`81a210ca98cc98402579144ab8845facf78b653b`. This is the finite blocker register
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
| B01 | Candidate run 38416920074 passes all 316 active cases in all 21 installed lanes; its saved reports independently replay unchanged truth. The ratchet retains two approved historical exclusions. Final combined-main acceptance remains open. | [#1974](https://github.com/elong0527/yamaa/issues/1974) |
| B02 | Public domain/check facades run on the shared engine. Installed signatures, argument errors, complete issues/log/output/save behavior and consumer migration remain. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B03 | Full-range R int64 and JSON-text context are approved. Lossless NUL-vector and retained-helper operations, deliberate removals and final installed surface qualification remain. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B04 | Declaring-file alignment, directory diagnostic priority and real Windows R symlink staging are merged (#2012/#2090). All active resource cases pass the complete candidate dispatch; renew combined-main evidence. | [#1995](https://github.com/elong0527/yamaa/issues/1995) |
| B05 | #2089 externally merges earlier probe/JSON retirement and the native R package. #2093 retains the final public filesystem and environment assertions; complete its installed matrix and land the last legacy IO consumer replacement before Python removal. | [#1990](https://github.com/elong0527/yamaa/issues/1990) |
| B06 | #1757 closed environment/function/codelist implementation. One complete engine use case must retain activation-before-data and all host effects in both bindings. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B07 | #1741 closed producer workflow implementation. Qualify its final composition with the unified use case, every port and failure/publication order. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B08 | Engine Define orchestration, registry ownership and domain submission metadata are merged (#2051/#2068). Renew final installed evidence after ownership and removal changes. | [#1986](https://github.com/elong0527/yamaa/issues/1986) |
| B09 | Correlated-intermediate acceptance #1979 is closed. The no-environment producer-schema candidate and all active corpus cases pass every installed lane. Finish additional #1980 source forms/grouped-filter disposition and #2005 predicate-site acceptance. | [#1975](https://github.com/elong0527/yamaa/issues/1975) |
| B10 | File/codec/publication implementations are merged. Final unified port composition must retain exact CSV, logical Parquet, original failures and held-save behavior. | [#1987](https://github.com/elong0527/yamaa/issues/1987) |
| B11 | Core/engine registry ownership, normative null dispositions and the empty adapter/binding literal allowlist are merged (#2084). Independent JSON truth preserves diagnostic assertions; renew current-head dispatch and combined-main evidence before #1984 closure. | [#1984](https://github.com/elong0527/yamaa/issues/1984) |
| B12 | #1740 closed the PortableLibmV1 numerical decision. Every affected supported language scope still needs exact public truth on the complete installed matrix. | [#1974](https://github.com/elong0527/yamaa/issues/1974) |
| B13 | Canonical Cargo locking, unchanged packaged workspace membership including build.rs and locked builds pass the audited full main run. Renew transitive-feature/toolchain and archive evidence on the final release revision. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B14 | Policy resolved: only packaging-tool checksum fields in python/uv.lock, rust/Cargo.lock and unchanged staged copies are allowed. Runtime/contract/cache/resource identity digests remain prohibited. Build evidence belongs to B13. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B15 | Full main run 38420713250 has 97 passing audit checks, independently replayed. Candidate dispatch 38416920074 passes all installed routes, strict R checks and all active public cases. Deferred shared Rust/math and final combined-main release qualification remain open. | [#1996](https://github.com/elong0527/yamaa/issues/1996) |
| B16 | Retained installed public suites cover ownership/interruption/reentrancy/repeated use, original failures and held Saves. Final full-matrix evidence and qualified Python legacy-surface removal remain. | [#1991](https://github.com/elong0527/yamaa/issues/1991) |
| B17 | Independent scalable workload generation and the installed collector are merged (#2003). Full collection, comparable baselines, phase/callback attribution and all-platform memory evidence remain incomplete. | [#1998](https://github.com/elong0527/yamaa/issues/1998) |
| B18 | Select numeric budgets after actual B17 measurements. Capacity refusals and unavailable metrics are not accepted timing/memory samples. | [#1973](https://github.com/elong0527/yamaa/issues/1973) |
| B19 | The concrete plan is merged (#2027). Eleven assets are retained in two unpublished rollback drafts with authenticated byte-equality receipts. The identical R source archive passes fresh installation on all three hosts; additional Python source/host proof, complete entry gates and budgets remain. Python reference removal #2110 and coordinated 0.4 preparation are gated candidates. | [#1999](https://github.com/elong0527/yamaa/issues/1999) |

## API and final audit

[release-api.json](release-api.json) inventories declared exports and their
dispositions; `python rust/tools/check_release_api.py` rejects source drift.
#1991 must update its pending classifications to the approved dispositions and
qualify attributes, signatures, result operations and retained helpers outside
the checkout. The inventory guard does not prove those behaviors.

#1997 now automates audits of the exact source revision/parents, every required lane, package
bytes, runtime/image/toolchain versions, numerical comparison and public/producer
reports. Missing, failed and unexercised tuples stay explicit. PR sampling is
feedback; full dispatched and main evidence supply qualification.

The historical assisted inventory and per-slice qualification ledgers are frozen.
Retire the assisted inventory only after #1974's full-matrix ratchet passes and
before deleting its probe consumers. Independent expected outputs remain fixed.
New evidence goes in the implementing PR and automated audit record; #1972 or
its wave-refresh successor owns this planning narrative.
