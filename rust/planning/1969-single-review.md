# Shared Rust migration review

[#2044](https://github.com/elong0527/yamaa/pull/2044) merged at
`14147ecfde9511fda9bf41840ae6306959c01667` on 2026-10-12. The maintainer requested
one subsequent PR, [#2058](https://github.com/elong0527/yamaa/pull/2058), to complete
the remaining migration and close its open work packages. Implementation,
qualification repairs, consumer changes and gated
cutover now accumulate on `codex/rust-migration-completion`. Keep that PR in draft
until its complete acceptance evidence is available. Superseded source ancestry
and qualification artifacts remain available; neither merging a component nor
closing a superseded PR closes a work-package issue by itself.

The combined branch preserves scalar and recursive source selections, source
options and ordinals, mapping and cut, dictionary resources, row and root
filters, output ordering, completed-template SELF donors, correlated
intermediate predicates, qualified and constructed-row aggregates, predicate
integer bounds, typed diagnostics, engine Define orchestration and ODM
expression ingestion. It incorporates the current review branches rather than
merging them individually to reduce the PR count.

Composition keeps the bounds-safe deferred-filter guard in every row mode,
filters candidates once before windows, and publishes SELF donors only after
the completed template. Aggregate failure context uses the unqualified group
field names required by the original reports. Diagnostic replay compares the
captured REQ-0068 citations directly; it does not rewrite the expected report
while executing a test. ODM ordinal declarations have one registry cause and
retain their complete REQ-1295 context and path.

Prior branch test runs and installed packages remain evidence for their exact
source revisions. They do not qualify the combined branch. Its acceptance
requires the complete workspace and test-support suites, strict Clippy and
repository guards, independently installed Python and R packages, the complete
public benchmark inventory and ratchet, the full dispatched native matrix,
and renewed combined-main evidence. Queued jobs supply no passing evidence.

The work-package issues and the
[transition entry criteria](transition.md) still govern remaining language,
public API, shared ownership, consumer, performance and retirement work. Keep
the reference evaluator and component consumers until their replacement
coverage and the prescribed cutover gates pass.

The completion PR follows the work packages in #1969. Its critical path is the
full #1974 inventory, completion of remaining language and shared ownership,
qualification of the public APIs and consumers, then performance, rollback and
reference retirement. Keep all changes in that PR; use separately validated
commits for the evaluator families described by the transition plan. A closure
comment must identify the implementing change and the complete acceptance
evidence. Keep unrelated issues outside the migration scope.
