# yamaa Rust migration status

Assessed 2026-10-13 against main
`0df4554fdf431133fd35c2f843ac0ff247eb9c3b`. The implementation and acceptance
owner is [#1969](https://github.com/elong0527/yamaa/issues/1969). This snapshot
records merged behavior and remaining gates; it is not release approval.
Earlier narrative remains in [the archive](archive/migration-status-before-work-packages.md).

## Verified public coverage

The last completed full main audit inspected here remains
[run 38394553441](https://github.com/elong0527/yamaa/actions/runs/38394553441),
source `a1a9c2d216075f331ac108aa4d3638573f103cea`. Its retained audit passes all
97 evidence checks. The newer complete installed dispatch
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
truth and ratchet evidence.

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
| Language acceptance | #1980: finish additional authored scalar/key source forms and the reviewed grouped-filter disposition (#2101/#2103). #2005: finish declared predicate-site acceptance. Candidate public corpus coverage is complete. |
| Diagnostic ownership | #1984: empty the remaining adapter/binding literal allowlist while preserving independent diagnostic truth. Core/engine ownership and normative null decisions have landed. |
| Binding and orchestration ownership | #1989 implementation is merged; renew combined-main acceptance. #1987's unified ordinary/environment/producer candidate passes the full installed dispatch. Qualify its supplied-environment diagnostic follow-up and final composition before closure. |
| Callback/probe cleanup | The R inheritance/1 callback is retired; #1988's remaining transports and #1990's unique assertions, obsolete exports and assisted consumers are addressed by the probe-removal candidate. Its cohort/audit replacement must reject malformed evidence. Diagnostic context remains JSON text. |
| Public surface and R package | #1991: complete the approved installed surface and deliberate removals; #1993: finish R/yamaa installation guidance, grammar replay and cdiscbuilder retirement. |
| Repository tooling | #2002: replace the temporary _reference_checks boundary and duplicate semantic checks with installed shared-engine checks; preserve structural validation and style/ODM-reader tooling. |
| Release evidence | #1996: final combined-main evidence, complete public corpus, Windows R resource behavior and locked package membership after the remaining changes. |
| Measurements | #1998 and #1973 item 10: collect the approved workload/size/form matrix, same-machine prior-release baselines, phase/callback/RSS evidence, raw samples and concrete passing budgets. The generator and collector exist; collection is incomplete. |
| Transition and removal | #1999 and #1973 item 11: approve the existing [transition plan](transition.md) only after all entry criteria and rollback rehearsals pass; #1994 then removes the reference and obsolete APIs, renewing all affected evidence. |

The maintainer delegated implementation decisions in
[maintainer-decisions.md](maintainer-decisions.md); no additional confirmation is
needed to continue independent implementation. Evidence-dependent budgets and
transition approval remain pending. The reference evaluator, assisted inventory,
component probe exports and R/cdiscbuilder remain in main. Their removal
candidates are not release approval.

Keep the assisted inventory frozen until its qualified public/typed replacements
permit retirement. Existing per-slice qualification ledgers remain historical.
New evidence belongs in implementing PRs and the automated main audit. See
[the blocker register](release-readiness.md) and [delivery order](migration-order.md).
