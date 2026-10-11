# yamaa Rust migration status

Assessed 2026-10-11 against main
`c8076646f403e2c15337c7771fa4341d2e5eaeee`. The current implementation and
acceptance owner is [#1969](https://github.com/elong0527/yamaa/issues/1969).
Its work packages replace the closed legacy umbrella and architecture issues.
This snapshot records merged behavior and remaining gates; it is not release
approval. Earlier narrative is preserved in
[the archive](archive/migration-status-before-work-packages.md).

## Merged implementation

The public Python `domain`/`check` and R `yamaa_domain`/`yamaa_check` use the
shared engine. The shared compiled model (#1752), environment/functions/codelists
(#1757), producer workflows (#1741), Define-XML/Dataset-JSON (#1758), numerical
policy (#1740), declaring-file paths (#1905), and bounded cohort (#1739) have
closed implementation owners. Their independent fixtures and retained reports
qualify their stated scopes. They do not substitute for complete public-corpus
or final combined installed-matrix evidence.

Cargo locking and source-package membership checks retain the canonical lock,
member manifests, build scripts and Rust sources. The approved first release
requires eighteen CPython 3.12-3.14 wheel/source lanes and three R 4.6.1 source
lanes on Linux x86_64, macOS arm64 and Windows x64. See
[the decisions](maintainer-decisions.md).

PR #2000 is merged at this assessment revision. It replaces the Windows R
UTF-8-sensitive handler-ledger regexp with literal truth comparison and adds
full-matrix branch dispatch. The dispatched head's Windows R job has passed;
its full dispatched run and combined-main run are still incomplete at this
assessment. #1970 remains open until both complete acceptance gates pass.

## Remaining implementation and qualification

| Scope | Work-package owners and concrete gate |
| --- | --- |
| CI prerequisites | [#1970](https://github.com/elong0527/yamaa/issues/1970): complete Windows R acceptance; [#1971](https://github.com/elong0527/yamaa/issues/1971): run feature-gated fake-port tests and document full dispatch. |
| Public conformance | [#1974](https://github.com/elong0527/yamaa/issues/1974): run every benchmark through both installed public APIs, compare independent truth, and seed a full-matrix ratchet. Counts come from that inventory, not component tests or static feature scans. |
| Language | [#1975](https://github.com/elong0527/yamaa/issues/1975)-[#1983](https://github.com/elong0527/yamaa/issues/1983): mapping/cut, ordering/input options, aggregates, scalar text/extremes, correlated intermediates, source/handler options, row derivations, ODM, and predicate/integer edges. [#2005](https://github.com/elong0527/yamaa/issues/2005): remaining predicate sites and verification literals. |
| Submission use case | [#1986](https://github.com/elong0527/yamaa/issues/1986): engine Define use case and specification submission metadata. |
| Semantic ownership | [#1984](https://github.com/elong0527/yamaa/issues/1984), [#1985](https://github.com/elong0527/yamaa/issues/1985), [#2004](https://github.com/elong0527/yamaa/issues/2004): diagnostics registry, literal guard and normative requirements; [#1989](https://github.com/elong0527/yamaa/issues/1989), then [#1987](https://github.com/elong0527/yamaa/issues/1987): translating bindings and one engine build use case. |
| Boundary cleanup | [#1988](https://github.com/elong0527/yamaa/issues/1988): retire R JSON inheritance callback; [#1990](https://github.com/elong0527/yamaa/issues/1990): migrate consumers and retire component probes. |
| Public API and consumers | [#1991](https://github.com/elong0527/yamaa/issues/1991): tested domain/check/define contract; [#1992](https://github.com/elong0527/yamaa/issues/1992): public benchmark runners; [#1993](https://github.com/elong0527/yamaa/issues/1993): R package/docs and deliberate cdiscbuilder retirement. |
| Repository tooling | [#1995](https://github.com/elong0527/yamaa/issues/1995): declaring-file policy in the validator; [#2002](https://github.com/elong0527/yamaa/issues/2002): validator/style decoupling before reference removal. |
| Final evidence | [#1996](https://github.com/elong0527/yamaa/issues/1996): Windows R, locked packages and final full matrix; [#1997](https://github.com/elong0527/yamaa/issues/1997): automated provenance/coverage audit; [#1998](https://github.com/elong0527/yamaa/issues/1998): representative cold/warm performance and memory. |
| Transition/removal | [#1999](https://github.com/elong0527/yamaa/issues/1999): concrete gated transition and rollback; [#1994](https://github.com/elong0527/yamaa/issues/1994): remove the reference and obsolete API after all entry criteria pass. |

Initial policy questions are tracked in
[#1973](https://github.com/elong0527/yamaa/issues/1973). Under the maintainer's
delegated decision authority, initial dispositions are recorded in PR #2006;
numeric budgets and transition approval still require their actual evidence.
The lossless R NUL-string representation remains the target.

The assisted inventory is historical and frozen. Retire it only after the
public conformance ratchet qualifies on the full matrix, before deleting its
probe consumers. All existing per-slice `*-qualification.md` ledgers are frozen;
new evidence belongs in the implementing PR and the #1997 audit record.

See the [finite blocker register](release-readiness.md) and
[delivery/evidence rules](migration-order.md). Update this snapshot at each
wave using #1972 or its explicit refresh successor; feature PRs do not own the
planning narrative.
