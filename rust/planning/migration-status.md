# yamaa Rust migration status

Assessed 2026-10-13 against main
`9f88cdb6b3855fdef2df8e8b208d2f948396edbc`. The implementation and acceptance
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

This complete ratchet and its last two active public runners are merged in
#2104. The merged main revision must still pass its own complete installed and
artifact audit; the candidate's recorded source is not relabeled as main.

The merged unified engine now admits producer-linked input schemas without an explicit
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

The external merges of #2091 and #2099 include unified route selection and the
bounded ordinary string-key index. #1987, #1984 and #1993 were closed externally
on 2026-10-13. Their scope implementation is merged; final combined-main
qualification remains a release gate. #2084's exact-head full dispatch
38433386841 passes every installed lane. Independent replay reproduces all 21
complete corpus verdict arrays against unchanged truth and confirms strict
Status: OK on all three R hosts.

The external merge of #2089 retires R/cdiscbuilder, points installation guidance
and shared grammar replay at R/yamaa, and includes the earlier probe-removal
slices. Main still lacks the last public filesystem controls retained in #2093;
its supplied-environment diagnostic follow-up is included in #2091.
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

| Scope | Owners and concrete remaining gate |
| --- | --- |
| Language acceptance | #1980: qualify the consolidated authored scalar/key and grouped-source forms in #2108. #2005: finish top-level assertions on declared selected lookup fields and their complete predicate-site acceptance. Candidate public corpus coverage is complete. |
| Diagnostic ownership | #1984 is closed externally; #2084's full installed matrix and independently replayed diagnostic corpus pass. Renew the combined-main audit with the remaining composition. |
| Binding and orchestration ownership | #1989 is closed with #2085's full-matrix evidence. #1987 is closed externally after the unified engine/lookup composition landed. Retain its full installed follow-up and final main audit as release evidence. |
| Callback/probe cleanup | #2089 includes the earlier endpoint and JSON-protocol retirement. #1988/#1990 still need #2093's final public filesystem and unique assertion controls in the complete installed matrix. Exact 90029d34 passes all three R source hosts, all twelve R supplemental suites, the 316-case ratchet and strict checks; eighteen Python lanes remain in qualification. Diagnostic context remains JSON text. |
| Public surface and R package | #1991: complete qualified Python legacy-surface removal and final installed contract evidence. #1993 is closed externally with #2089's R/yamaa guidance, grammar replay and cdiscbuilder deletion. Full-range integers, public lifetime/interrupt and filesystem suites pass all three R hosts at 90029d34; final versioned package acceptance remains. |
| Repository tooling | Native CSV and regex validator routes and the retained style CLI are merged (#2095/#2097/#2087). #2111 at f00cffdc retains all 420 original validator controls, 1,919 passing shared Rust tests, 491 engine feature tests and 77 active native fixture proofs plus two historical exclusions. Its exact full matrix 38458966887 includes mandatory real-R issue transport; that acceptance is pending. #2109's changed numerical tools pass on all three hosts and await the complete installed composition. |
| Release evidence | #1996: final combined-main evidence, complete public corpus, Windows R resource behavior and locked package membership after the remaining changes. |
| Measurements | #1998 and #1973 item 10: complete the approved workload/size/form matrix, comparable baselines and passing budgets. #2094 merges the retained collector; #2117's unapproved capacity/phase prototype passes seven ordinary 100k cases and callback observation, while two producer forms refuse at a separately unwired CSV limit. Actual report framing/copy scope and largest-size producer acceptance remain under review. No capacity or numerical budget is approved. |
| Transition and removal | #1999 and #1973 item 11: approve the [transition plan](transition.md) only after all entry criteria pass. Two unpublished rollback releases retain eleven byte-verified assets. The canonical R archive passes all three hosts; exact prior Python wheel/source replay on eighteen tuples is running separately. #2110 prepares Python removal and #2113 prepares coherent 0.4 versions/notes; both remain gated candidates. |

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
