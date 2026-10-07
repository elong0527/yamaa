# Shared Rust migration implementation order

Reconciled 2026-10-07 against main `2bfbc01ae414e0fc8fb6660c48e91947c6231e72`
and issues #1585, #1739-#1742, #1751-#1755, #1757 and #1758.
This is a delivery plan, not a language change or a qualification claim.

## Goal and starting point

Deliver one authoritative Rust implementation behind the Python and R APIs in
#1751 and #1758, including original-YAML compilation, workflows, project functions,
terminology, submission output and release qualification. Complete the supported
benchmark/API matrix before cutover and remove duplicate implementations only
after their replacements qualify. PRs proceed serially and merge only after
applicable final-revision CI, full base-to-head review and a fresh merge guard.

The old plan's next step, structural admission, already shipped in #1747.
#1750 added original division-by-zero and integer-overflow runs in both hosts.
#1756 added the unchanged ordered-sum document, its 17 rows, verification and exact
1,030-byte CSV. These three original-document
integrations are supplemental evidence: the benchmark inventory still records
the six-case cohort as reference-assisted, not `shared_run`.

#1739 was closed by #1756's merge despite that PR explicitly retaining its
unfinished acceptance gates. It remains open: the production frontend and complete
cohort inventory qualification have not passed. Counts of component tests,
merged PRs or closed issues are not migration percentages.

Subsequent reviewed and installed-package-qualified slices have merged:

| PR | Delivered scope |
| --- | --- |
| #1761 | Portable diagnostic foundation. |
| #1762 | Shared core compiled representation. |
| #1763 | Engine capture/codec/build ports with retained failure observations. |
| #1764 | Original lookup document integration. |
| #1767 | Complete original window document integration. |
| #1768 | Original inherited `spec_study.yaml` integration. |
| #1769 | Shared preflight/build use case with rejection before study reads. |
| #1770 | Package-owned shipped schema and automatic raw-document preparation. |
| #1771 | Owned admitted build output and explicit save without recapture. |
| #1772 | Portable captured schema, decode and inherited preparation findings. |
| #1773 | Classified source-capture findings and owned failed-build reports. |

All six original cohort documents now pass the private installed entry points in
both hosts, with exact complete reports and publication bytes. This does not yet
qualify the public domain/check facade or replace reference preparation in the
conformance runner. The existing inventory still records all six as
`reference_assisted_run`.

The current delivery frontier is reviewed declaring-file path rules, production
file ports and resource preflight, and the bounded public facade and conformance
frontend. Schema findings retain standalone and inherited preparation context;
classified capture failures retain owned failed-build reports. Actual file-port
integration and complete failure-family coverage remain required. The three public
representation decisions below remain open. Do not substitute another round of
component-only evidence for the remaining original-YAML frontend gates.

## Serial delivery order

| Order | Issues | Deliverable and exit gate |
| --- | --- | --- |
| 1 | #1753, then #1752 | Establish portable diagnostics and a core-owned immutable compiled specification. Migrate one exercised family at a time, retaining exact independent outcomes; make the existing three original-document runs use the same representation as the temporary dataset protocol. |
| 2 | #1755; #1754 alongside each moved service | Define the engine-owned resource, codec, publication, environment and function ports and shared fakes. Move semantic decisions out of adapters as those services migrate. Preserve failure/read/publication order; do not build new features on JSON host callbacks or adapter-owned planning. |
| 3 | #1751 foundation, #1739 | Finish the reviewed path/API rules, file ports, failure reports and bounded `domain`/`check` facade over the landed shared schema/compiler services. Connect the original-YAML conformance frontend for the now-implemented lookup, full windows and inherited entrypoint. Qualify all six original documents through both installed hosts and promote only evidenced `shared_run` tuples. |
| 4 | #1757 | Land environment/function/codelist rules and schema, shared static admission, lock verification and per-build function testing before study reads; migrate the five function benchmarks and equivalent R projects. Use the reviewed new format rather than extending the retired artifact/cache design. |
| 5 | #1741 | Qualify the producer workflow on the compiled representation and ports, including activation-before-data, producer-once execution, rounded serialized consumer inputs, failure ledgers and explicit save gates. |
| 6 | #1758 | Complete shared Define-XML and Dataset-JSON generation from environment sections, migrate the three submission benchmarks and only then retire `define.yaml` and its loaders/schema. Preserve the existing committed XML/JSON bytes for fixed creation time. |
| 7 | #1585, #1751, #1754 | Close remaining language-family gaps from the executable inventory; qualify the full public API matrix, finish adapter cleanup, migrate benchmark runners/docs and remove superseded APIs/probes after their replacements pass. |
| 8 | #1742, #1585 | Complete release/build reproducibility, installed support matrix, representative performance/memory evidence and approved budgets; perform the qualified transition and remove duplicate evaluators. |

This order is incremental, not an all-or-nothing prerequisite chain. In step 1,
the diagnostic type and first exercised mappings unblock the compiled model;
converting every diagnostic family continues as those families migrate. Step 2's
port interfaces and build services unblock the bounded facade; function/environment
port implementations finish in step 4. #1754 cleanup follows moved services and
does not require refactoring probe endpoints scheduled for removal.

#1751 has an early bounded-facade gate and a later full-API gate. Requiring its
entire benchmark migration before #1739, while requiring #1739 before the facade,
would create a false cycle. Similarly, #1752's representation/binding foundation
precedes the remaining compiler extensions, while its six-document acceptance
finishes with #1739. Pure compilation receives source schemas at the required
ingestion boundary; it must not move source-dependent failures before ingestion.

## Decisions and preparation that start early

These tasks can be prepared between serial implementation PRs without holding up
the non-transcendental cohort:

- **#1740:** retain the existing numerical evidence and compatibility witnesses,
  and use the reconciled #1751 proposal: one numerical behavior per locked yamaa
  release. The old per-run selectable policy proposal is not the target public
  API. Record the maintainer's numerical choice before changing production
  behavior, then qualify it in both hosts and every affected compute context.
- **#1742:** maintain the [release blocker register and declared API inventory](release-readiness.md),
  using the API replacement in #1751 rather than assuming legacy API preservation.
  Reproducible Cargo dependencies, Windows R scope and numeric performance budgets
  remain explicit decisions. Measure representative shared workloads once their
  compiler/workflow paths exist; tiny fixtures cannot support speedup claims.
- **#1751:** record the three public representation decisions: `check`'s result,
  R's full-range integer behavior and `issues.context`. The current full-i64
  contract cannot silently lose INT64_MIN through bit64's missing-value sentinel.
- **#1757:** the proposed study/benchmark uv/renv lock-file checksum exception
  needs explicit maintainer approval. Until then, AGENTS.md still permits only
  `python/uv.lock`. Do not introduce other digest fields or weaken that convention.
  Rules/schema and static environment work can proceed independently.

Coordinate the environment submission classes in #1757 with #1758. Adding the new
classes does not permit deleting the old Define-XML schema or loaders before the
replacement generator and migrated benchmarks pass. Likewise, path policy,
function activation and numerical policy changes need reviewed normative changes
and focused tests, not silent edits during an architectural move.

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
  lock. Cutover remains gated by #1742 and the umbrella acceptance criteria.
- Keep #1585 open through complete supported scope and transition. Bounded
  milestones and decision preparation alone cannot close the migration.

The current unattended-work authorization permits creating and merging PRs. It
does not supply answers to the explicitly open numerical/lock-file decisions.
Continue independent work and retain those blockers until they are resolved.
