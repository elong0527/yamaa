# Shared Rust migration implementation order

Reconciled 2026-10-08 against main `514c0725f9f66a376dec9db2f15fde6d0e254ce7`
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
| #1776 | Strict hosted R package checks with default package-local Rust build cleanup. |
| #1777 | Bounded Parquet output and explicit retained save. |
| #1783 | Reference Parquet invalid-UTF-8 ingestion classification. |
| #1785 | Held Parquet ingestion through shared source ports, qualified on all prototype package forms. |
| #1786 | Core original-document preflight and output-declaration diagnostics, with complete retained failed reports. |
| #1788 | Core numeric/aggregate grammar diagnostics and independent original-document failure timing. |
| #1789 | Portable schema kinds, core binding/dependency findings and pure core CSV profile admission. |
| #1790 | Original scalar column literals and column/row conversion recovery wrappers. |
| #1791 | Portable window/lookup/predicate findings and three unchanged negative corpus reports. |
| #1792 | Pure core declared CSV source typing and closed Parquet metadata admission. |
| #1793 | Bounded Python file capture and classified host source replies with retained exception/condition identity. |
| #1794 | Declaration-order metadata inspection before every study capture, with complete zero-read failures. |
| #1795 | Driver source filters and ordered first_available with complete original source failures. |
| #1798 | Original dataset assertions and all_or_none with complete reports and unchanged paired-date failure. |

All six original cohort documents now pass the private installed entry points in
both hosts, with exact complete reports and publication bytes. This does not yet
qualify the public domain/check facade or replace reference preparation in the
conformance runner. The existing inventory still records all six as
`reference_assisted_run`.

The held Parquet input implementation in #1785 merged after final-revision
installed qualification and review, including all 52 independent containers and
nineteen complete negative reports. It qualifies the private codec/build/save
route, while actual filesystem ports and inventory promotion remain open.
Original-document preflight, output-declaration and numeric/aggregate grammar
findings now originate as core diagnostics; other error families and cross-protocol serialization still
need conversion. Their common projection preserves the existing complete truth.
#1788 passed all 13 checks and its final installed-artifact audit, including six
Python forms with 29 original-document methods and both strict R source checks.
The Ubuntu R job passed an isolated retry after dependency-mirror delays.
#1789 passed all 13 checks and final-head installed-artifact auditing: six Python
forms with 31 original-document and four schema methods, plus both strict R source
packages with all 17 scripts. The reference R check passed an isolated retry after
setup timed out before tests. Its 58-contract/39-suite catalog and unchanged
945-row assisted inventory are qualified private-route evidence.
#1790 merged as `6ac27ce75f6758a68275eabf0753702234a29f8d` after all 13
checks, full final-head review and downloaded installed-artifact auditing. Six
Python forms pass 22 supplemental suites, 34 original-document methods and four
schema methods each; both R source forms pass all 17 scripts with strict Status:
OK. All native artifacts are from attempt 1. The 61-contract/39-suite catalog and
unchanged 945-row assisted inventory retain private-route evidence.

#1791 merged as `1c06fc5c8c0e2f72283d11a52db8426baa2db701` after all 13
checks, full final-head review and downloaded installed-artifact auditing. Six
Python forms pass 22 supplemental suites, 36 original-document methods and four
schema methods each; both R source forms pass all 17 scripts with strict Status:
OK. All native artifacts are from attempt 1. Twenty changed Rust/source/fixture
and twenty-one R archive members match directly. Its 66-contract/39-suite catalog
now covers nine original corpus documents through the private route. Three
separately observed predicate requirement/context gaps remain unqualified and
documented in `predicate_diagnostics.md`; locked truth and assisted inventory
remain unchanged.

#1792 merged as `fc7eca7fdd96d828ce5234d1d4204b098f301988` after all 13
checks, full final-head review and downloaded installed-artifact auditing. All six
native jobs passed attempt 1. Six Python forms pass 22 supplemental suites, 37
original-document and four schema methods each; both R source forms pass all 17
scripts with strict Status: OK. Seventeen changed native Python and eighteen R
members, plus all 52 independent Parquet binaries in every source archive, match
directly. Its 67-contract/39-suite catalog and 75 reached canonical causes retain
private-route qualification, with the 945-row assisted inventory unchanged.

#1793 merged as `a6bcd5dbae458fec96998d5f257042b216463d2b` after all 13
checks, full final-head review and downloaded installed-artifact auditing. Native
run 37706393083 passed all six jobs on attempt 1, including portable math. Six
Python forms pass 23 supplemental suites, 39 original-document, four actual-file
and four schema methods each; both R source forms pass all 17 scripts with strict
Status: OK. Exact host/native source bytes and all 52 Parquet binaries match.
Its 68-contract/40-suite catalog and unchanged 945-row assisted inventory retain
private-route qualification. Python capture and verification are bounded; the
existing approved-root/link policy remains pending explicit path approval.

#1794 merged as `a9d27c06ac9ba266cff59fe6190ed77c7fda01ca` after all 13
checks, full final-head review and downloaded installed-artifact auditing. Native
run 37709951425 passed all six jobs on attempt 1. Six Python forms pass all 23
supplemental suites, 42 original-document, six actual-file and four schema methods;
both strict R source forms pass 17 scripts with Status: OK. Its
69-contract/40-suite catalog, exact source bytes, all 52 independent Parquet
binaries and unchanged 945-row assisted inventory were audited. Metadata-only
inspection of all declared sources precedes every study capture in the engine;
Python's actual filesystem adapter opts in. Existing explicit-byte callbacks may
omit inspection and retain their qualified timing. Actual R filesystem adapters,
environment/function preflight and public frontend/API qualification remain open.

#1795 merged as `37c16057313b5fd5e5f0ab2de818ed124e6d0d81` after all 13
checks, clean full final-head review and downloaded installed-artifact auditing.
Native run 37713778588 passed all six jobs on attempt 1. Six Python forms pass
23 supplemental suites, 44 original-document, six actual-file and four schema
methods; both R source forms pass 17 strict scripts with Status: OK. The full
27-file review caught and verified an R guard correction: all 24 selection cases
run with Python absent from PATH. Its 73-contract/40-suite catalog, exact changed
archive members, all 52 Parquet fixtures and unchanged 945-row assisted inventory
match. Both unchanged negative source benchmarks qualify privately, bringing the
original corpus to eleven. Filtered keys, source ordering, secondary/intermediate
selection and existing predicate diagnostic gaps remain open.

#1798 merged as `514c0725f9f66a376dec9db2f15fde6d0e254ce7` after all 13
checks, clean full final-head review and downloaded installed-artifact auditing.
Native run 37717106416 passed all six jobs on attempt 1. Six Python forms pass
23 supplemental suites, 46 original-document, six actual-file and four schema
methods; both R source forms pass all 17 strict scripts with Status: OK and Python
absent from PATH. Its 76-contract/40-suite catalog, exact changed archive members,
all 52 Parquet fixtures and unchanged 945-row assisted inventory match. Twenty-eight
independent complete assertion/all_or_none reports preserve six exact successful
CSV cases. The unchanged negative-paired-dates document qualifies privately,
bringing the original corpus to twelve. [Final evidence](https://github.com/elong0527/yamaa/pull/1798#issuecomment-6051139650).
Warning severity, column checks and special predicate gaps remain open.

The original column presence slice is merged in [PR #1799](https://github.com/elong0527/yamaa/pull/1799)
at `63722ee7e616ed8c1da594bb6b4eaf2ccc15a327`. Its exact final head passes all
13 checks and full 22-text-file review with no actionable comments. Fresh native
run 37719793926 passes all six jobs on attempt 1. Every Python platform's direct
and source-rebuilt form passes 23 suites (47 original-document, six actual-file
and four schema methods); both R forms pass 17 strict scripts with Status: OK and
Python absent from PATH. The 78-contract/40-suite catalog, changed archive members,
all 52 Parquet fixtures and unchanged 945-row assisted inventory match. Ten
independent complete presence reports retain four exact CSV successes, and the
unchanged negative-not-missing-age document brings the private original corpus to
thirteen. [Final evidence](https://github.com/elong0527/yamaa/pull/1799#issuecomment-6051580788).

The next prepared compiler candidate admits error-severity column allowed_values,
range and max_length at the same ordered checkpoint. Missing values pass; allowed
literals convert once to the declared kind, mixed integer/float bounds compare
exactly, and text length counts Unicode scalars. Twenty independently authored
complete reports preserve deferred declaration findings, completed prefix records,
cached captures and six exact successful CSV cases. UTF-8 fixture readers compare
Rust's canonical report text in both installed hosts. The unchanged
negative-implausible-age and negative-invalid-sex reports bring the locally
qualified original corpus to fifteen. Matches, warnings and row-template column
checks remain Unsupported before study capture.

Local immutable runtime `808e331ac7e0dafef1cdc5897e9b939069ce6833` passes
687 Rust tests across 78 targets, strict Clippy, 44 tooling tests, both installed
Python forms with 23 suites (48 original-document, six actual-file and four schema
methods), and all 17 strict R scripts with Status: OK. R replay stays inside the
Python-free guard. Sixteen changed native Python and seventeen R archive members,
plus all 52 Parquet fixtures, match directly. Its 81-contract/40-suite catalog and
82 reached canonical causes await final main-based review, hosted qualification
and downloaded artifact auditing. No assisted inventory tuple or public frontend
gate is promoted by this prepared candidate.

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
