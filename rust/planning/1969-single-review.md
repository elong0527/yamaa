# Shared Rust migration review

The remaining migration implementation is reviewed in
[#2044](https://github.com/elong0527/yamaa/pull/2044), following the maintainer's
request to combine recent unmerged PRs. Further fixes and implementation update
that PR. Superseded branches, source ancestry and their qualification artifacts
remain available; closing a superseded PR does not close its work-package issue.

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
