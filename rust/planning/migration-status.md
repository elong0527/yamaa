# yamaa Rust migration status

Assessed 2026-10-13 against main
`81a210ca98cc98402579144ab8845facf78b653b`. The implementation and acceptance
owner is [#1969](https://github.com/elong0527/yamaa/issues/1969). This snapshot
records merged behavior and remaining gates; it is not release approval.
Earlier narrative remains in [the archive](archive/migration-status-before-work-packages.md).

## Verified public coverage

The last completed full main audit inspected here is
[run 38420713250](https://github.com/elong0527/yamaa/actions/runs/38420713250),
source `c42788b2ffc6025c76d699e3794ab4ee70d9fe6e`. Its retained audit and
independent replay pass all 97 evidence checks. All 21 saved main inventories
also replay the same complete verdicts against unchanged expected outputs:
312 Python/Unix R passes and 311 Windows R passes, with the previously recorded
refusals and path mismatches. The separate complete installed dispatch
[run 38416920074](https://github.com/elong0527/yamaa/actions/runs/38416920074),
source `2ba10418607933f92597f7c00ac363a3113a49db`, qualifies the unified-engine
candidate. All 21 saved public inventories were independently replayed against
unchanged committed expected outputs before advancing the ratchet below.

| Installed routes | Pass | Mismatch | Unsupported | Error | Historical exclusions |
| --- | ---: | ---: | ---: | ---: | ---: |
| All 18 Python interpreter/platform/form routes | 316 | 0 | 0 | 0 | 2 |
| All three R source-package routes | 316 | 0 | 0 | 0 | 2 |

The ratchet is the intersection: **316 Python and 316 R benchmarks out of 318**.
Every active case passes; the two approved historical path exclusions remain
explicit. No previous passing case is removed. This is complete installed corpus
evidence for the candidate revision. Its deferred shared Rust and numerical
checks and the final combined-main audit remain separate acceptance gates.

The candidate now admits producer-linked input schemas without an explicit
environment for `adam-adsl-randomization`, `negative-redefined-date` and
`sdtm-dm-race-ethnicity`. The merged resource-kind priority and Windows R staging
changes (#2090) preserve the directory and real-symlink diagnostic truth. The
two newly qualified positive runners use the public `domain` API. None of these
changes modifies expected outputs.

## Merged implementation

Both public hosts execute domain/check/define through the shared Rust engine.
The shared compiled model, environment/functions/codelists, producer workflows,
Define-XML/Dataset-JSON, numerical policy, declaring-file paths and bounded cohort
have merged implementation. Later work added mapping/cut, input options and
ordering, aggregates, scalar text/extremes, ODM, row derivations, submission
metadata and correlated intermediate derivations with eager verification.
Binding-to-core dependencies and duplicated binding policy are removed (#2085),
and the R JSON inheritance callback is replaced by a typed engine outcome
(#2086). Correlated-intermediate acceptance (#1979) is closed with its full-matrix
truth and ratchet evidence. Translating-binding acceptance (#1989) is closed with
the exact-head full matrix and the independently replayed complete main audit.

The external merge of #2089 retires R/cdiscbuilder, points installation guidance
and shared grammar replay at R/yamaa, and includes the earlier probe-removal
slices. That merge does not supply final qualification: main still lacks the
last public filesystem and supplied-environment controls retained in #2093.
The installed Python filesystem suite on this main revision still imports the
legacy IO helper; its public replacement must land before Python removal.

The first release still requires CPython 3.12-3.14 wheel and independent source
rebuilds on Linux x86_64, macOS arm64 and Windows x64, plus R 4.6.1 source builds
and strict R CMD check on all three hosts. Canonical Cargo locking and staged
workspace membership remain enforced. The retained public contract suites and
Windows R installed suites pass in the audited run above.

The CI policy merged in #2080 defers shared Rust checks and exact cross-platform
math assessment to main pushes. Complete manual PR-head dispatch still qualifies
all 18 installed Python routes, three R hosts and public/producer cohorts before
merge. Complete work-package acceptance uses the combined main audit, including
the deferred Rust and numerical evidence. See [CI instructions](../../.github/ci.md).

## Remaining work

| Scope | Open owners and concrete remaining gate |
| --- | --- |
| Language acceptance | #1980: qualify the consolidated authored scalar/key and grouped-source forms in #2108. #2005: finish top-level assertions on declared selected lookup fields and their complete predicate-site acceptance. Candidate public corpus coverage is complete. |
| Diagnostic ownership | #2084 empties the adapter/binding literal allowlist using independent JSON truth. Core/engine ownership and normative null decisions are merged; #1984 still requires the current-head full dispatch and combined-main evidence. |
| Binding and orchestration ownership | #1989 is closed with #2085's full-matrix evidence. #1987's unified ordinary/environment/producer candidate passes the full installed dispatch. Qualify its supplied-environment diagnostic follow-up and final composition before closure. |
| Callback/probe cleanup | #2089 includes the earlier endpoint and JSON-protocol retirement. #1988/#1990 still need #2093's final public filesystem, supplied-environment and unique assertion controls in the complete installed matrix. Diagnostic context remains JSON text. |
| Public surface and R package | #1991: complete qualified Python legacy-surface removal and final installed contract evidence. #1993's R/yamaa guidance, grammar replay and cdiscbuilder deletion are merged in #2089; its final package matrix remains pending. |
| Repository tooling | Native CSV and regex validator routes and the retained style CLI are merged (#2095/#2097/#2087). #2111 retains all 420 original validator controls, 1,907 passing Rust tests and 77 active native fixture proofs plus two historical exclusions. Its exact full installed matrix includes mandatory real-R issue transport; that acceptance is pending. #2109's changed numerical tools pass on all three hosts and await the full installed matrix. |
| Release evidence | #1996: final combined-main evidence, complete public corpus, Windows R resource behavior and locked package membership after the remaining changes. |
| Measurements | #1998 and #1973 item 10: collect the approved workload/size/form matrix, same-machine prior-release baselines, phase/callback/RSS evidence, raw samples and concrete passing budgets. Actual phase and cumulative-work prototypes pass functional controls; largest-size resource acceptance and distributions are incomplete. |
| Transition and removal | #1999 and #1973 item 11: approve the existing [transition plan](transition.md) only after all entry criteria pass. Two draft rollback releases retain eleven assets with authenticated byte-equality receipts; the R canonical source archive passes all three hosts, while remaining Python source/host rollback proof is pending. #2110 prepares Python removal separately. |

The maintainer delegated implementation decisions in
[maintainer-decisions.md](maintainer-decisions.md); no additional confirmation is
needed to continue independent implementation. Evidence-dependent budgets and
transition approval remain pending. The Python reference evaluator and legacy
exports remain in main. The externally merged R/probe removals and the remaining
Python removal candidate are not release approval.

Keep the assisted inventory frozen until its qualified public/typed replacements
permit retirement. Existing per-slice qualification ledgers remain historical.
New evidence belongs in implementing PRs and the automated main audit. See
[the blocker register](release-readiness.md) and [delivery order](migration-order.md).
