# yamaa Rust migration delivery order

Assessed 2026-10-13 against main
`9f88cdb6b3855fdef2df8e8b208d2f948396edbc`. The authoritative work packages,
dependencies, waves and conflict groups are in
[#1969](https://github.com/elong0527/yamaa/issues/1969). Use that issue's current
table when selecting work. The earlier serial plan is preserved in
[the archive](archive/migration-order-before-work-packages.md).

The CI prerequisites, public conformance inventory and most language families
have landed. The merged ratchet requires 316 Python and 316 R benchmarks out
of 318, with every active case passing and two approved historical exclusions.
The binding work package is closed; unified orchestration, diagnostic ownership
and R retirement are merged and their work packages were closed externally.
R retirement and earlier probe-removal slices landed in
#2089. Finish their combined-main acceptance, the final public filesystem and
environment controls, source-option and lookup-predicate acceptance, and the
final unified engine composition. Qualify the decoupled repository validator and final
public surface before Python retirement. Collect performance baselines before
removing the reference.
The transition plan is merged; approval and removal still require its concrete
entry criteria and complete combined-main evidence.

Implementation may proceed before its conformance acceptance dependency closes;
acceptance may not. Rebase shared-file changes onto current main before opening
and merging their PRs. Native/R/packaging/CI changes require complete dispatched
PR-head installed qualification, including Windows R, followed by combined-main
acceptance. Under #2080, shared Rust checks and the exact math assessment run on
main pushes; a PR dispatch alone cannot establish their acceptance.
See [the dispatch instructions](../../.github/ci.md).

The [decision record](maintainer-decisions.md), [status](migration-status.md) and
[blocker register](release-readiness.md) distinguish target policy from observed
qualification. Existing per-slice qualification ledgers are frozen; record new
evidence in PRs and the #1997 audit instead of adding overlapping narratives.

## Evidence and completion rules


- Preserve current committed expected outputs. Any intentional semantic change
  needs separately justified truth and reviewed rules; never regenerate goldens
  to fit the candidate or use a tolerance to hide observable numerical changes.
- Keep Unsupported before study effects and preserve required ingestion, binding,
  derivation, verification and publication order. No midway reference fallback.
- Qualify direct and source-rebuilt Python packages and R source packages outside
  the checkout on the declared matrix, with Python semantics disabled and R free
  of a Python runtime requirement. Retain original host errors and interruptions.
- Record tested revision, package form, platform, report/artifact identity and
  coverage level. Inspect required installed evidence; passing component tests
  do not establish a complete-run qualification claim.
- Preserve a passing reference baseline during migration. Its temporary presence
  is a verification aid, not a commitment to retain a public backend selector.
  The target rollback in #1751 is pinning the previous yamaa release in the host
  lock. Cutover remains gated by #1999 and the #1969 umbrella acceptance criteria.
- Keep #1969 open through complete supported scope and transition. Bounded
  milestones and decision preparation alone cannot close the migration.
  The superseded #1585 issue retains historical evidence.

The maintainer's recorded decisions supply the implementation choices above.
They do not replace final qualification or explicit release-transition approval.
Continue independent implementation and retain every remaining acceptance gate
until its linked evidence or scope disposition is complete.
