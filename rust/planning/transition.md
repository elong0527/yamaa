# yamaa shared Rust release transition

This is the execution plan for [#1999](https://github.com/elong0527/yamaa/issues/1999)
and [#1994](https://github.com/elong0527/yamaa/issues/1994). The maintainer
authorized the implementation agent to approve decisions and complete the
migration. Merging this plan records the process; transition approval remains
pending until the combined main revision satisfies every entry criterion below.
Do not remove the reference evaluator or publish the release before that approval.

## Entry criteria

The candidate is one immutable main revision and its built package artifacts.
Evidence from separate branches is development evidence, not acceptance of their
composition. Any source, dependency, toolchain or package change invalidates the
affected evidence and requires renewed qualification.

| Gate | Required evidence and owner |
| --- | --- |
| Language | #1975-#1983, #1986 and #2005 have no remaining unsupported active language scope. Newly reached cases preserve independent benchmark truth. |
| Public conformance | #1974 enumerates every active benchmark, compares complete outputs and diagnostics in both installed hosts, and ratchets every pass. No unsupported, mismatch, error, skipped active fixture or unexercised tuple remains. Approved historical exclusions retain their explicit disposition. |
| Public API | #1991 qualifies retained APIs, argument errors, output/issues/logs, full i64 and lossless NUL values, ownership, interruptions, reentrancy, repeated use, Define and held Save. #1973's retained ODM helpers also qualify. |
| Shared ownership | #1984/#1985 own diagnostics in core/engine; #1987 composes the same ports and route selection for both hosts; #1989 removes binding semantics and duplicate limit checks. |
| Callback and probe retirement | #1988/#1990 replace every unique probe assertion with public or typed-native coverage and retire JSON host protocols in the prescribed order. Diagnostic context remains JSON text. |
| Consumers | #1992 switches only ratcheted Python runners and eventually every active runner. #1993 documents R/yamaa and preserves grammar replay. #2002 removes reference imports and duplicate semantic checks from repository validation. |
| Installed matrix | A full dispatched run on the candidate passes all eighteen Python interpreter/platform/form tuples and all three R source-package tuples, including strict R CMD check. #1971/#1970 prerequisites and #1996 release evidence are complete. PR sampling does not substitute. |
| Audit | #1997 retains the exact revision and parents, package bytes, Cargo workspace/lock membership, runtime/image/toolchain versions, numerical reports and complete public/producer reports. Missing artifacts fail acceptance. |
| Performance | #1998 covers lookup, aggregate, window, callbacks and producer graphs at 1k/10k/100k driving rows on every approved platform/form. Independent truth passes before each sample counts. Retain ten cold samples and thirty warm samples after three warm-ups, raw spread, phase/callback/RSS observations and a same-machine prior-release baseline. #1973 item 10 records concrete budgets and their passes. Unavailable metrics and capacity refusals are explicit findings. |
| Decision record | #1973's implementation choices are recorded in maintainer-decisions.md; item 11 links the complete combined-main evidence and explicitly approves the transition. Every B01-B19 row has a completed disposition. |
| Rollback rehearsal | A fresh isolated environment installs the retained prior-release artifact for each required host/form and replays representative specifications and outputs. Verify the exact version before reading study data or invoking callbacks. |

Active benchmark inventory comes from the reviewed manifest and #1974 report.
Do not improve coverage by dropping a failing fixture or changing expected bytes.
The four historical expected/spec_resolved.yaml files have no retained public
producer. Their exclusion and removal follow #1973 item 14. The two old path
containment negatives follow the approved declaring-file policy and their
reviewed historical disposition.

## Release sequence and opt-in

The current published Python release is 0.3.0, tag yamaa-py-0.3.0 (checked
2026-10-11). The planned shared release is **0.4.0** for Python and R, with the
embedded Cargo workspace version advanced to **0.4.0** in the same release PR.
This identifies the deliberately changed API and numerical behavior with one
locked yamaa release. If that version is already published when packaging begins,
select and record the next unused minor version before changing any manifests.
No version file or published asset changes as part of this plan.

1. Preserve and qualify the 0.3.0 rollback packages and the reference measurement
   environment before deleting any code. Keep their actual package bytes and
   exact host locks; an R source artifact must exist and pass the rehearsal before
   promising R rollback. Preserve the old API usage required by that release.
2. Complete and record the entry gates on main. The implementation agent posts
   the concrete item 11 approval in #1973, linking the audit and budgets. An open
   gate postpones the transition without asking for repeated maintainer approval.
3. Apply the removal PRs below, one reviewed family at a time. After each merge,
   run repository validation, release/API/dependency/diagnostic guards and the
   full installed matrix with the public ratchet. Stop on the first failure.
4. Prepare the coordinated version/notes/distribution PR. Stage unchanged
   canonical Cargo.lock and the complete Rust workspace into both source
   packages. Rebuild independently from source, verify package membership and
   repeat the full matrix and performance gates on the final versioned revision.
5. Publish only the qualified artifacts from that revision. Python supplies the
   qualified wheel and source distribution. R supplies the staged source tarball
   as a GitHub release asset under #1973 item 13, with source-install instructions
   and its required Rust toolchain. CRAN and r-universe are later distribution work.
6. Users opt in by updating their Python or R lock to the new release in a fresh
   environment, reviewing the API/numerical notes, then starting a new run. There
   is no public backend selector, runtime fallback or per-operation policy switch.

Release notes list the retained domain/check/define surface, removed APIs and
replacement calls, supported matrix and source build requirements, declaring-file
paths, full-range integer/NUL carriers, and retained ODM readers. Include the
[PortableLibmV1 compatibility decision](1740-math-decision.md): EXP/LN/POWER bits,
downstream predicates, conversions and serialized output can differ from the
historical platform implementation. Do not promise mathematical superiority,
universal correct rounding or historical bit compatibility.

## Version reporting

Python yamaa.__version__ reports installed distribution metadata. R
utils::packageVersion("yamaa") reports its installed package version. Packaging
checks require both to match the selected release and their own installed
artifacts, outside the checkout. The embedded core version is pinned by the same
release and recorded in native qualification/audit metadata and package provenance.
Keep its internal reporting for diagnostics and qualification; do not retain the
legacy public engine_info probe or add a backend/policy selector.

Version metadata is distinct from runtime artifact identity. Runtime artifacts
remain pinned by runtime.artifact.reference alone; contracts use canonical JSON
directly and resource snapshots retain bytes. No new digest identity fields are
introduced. Packaging-tool lock checksums retain the repository's narrow exception.

## Rollback before a new run

Stop the application and discard outstanding build/result handles. In a fresh
environment, restore the reviewed prior-release lock: Python pins yamaa==0.3.0
in project dependencies and restores/rebuilds uv.lock; R restores renv.lock and
the retained qualified 0.3.0 source artifact. Restore the compatible calling code
and required runtime artifacts as well. Verify package versions and the selected
input/specification snapshots before starting a new build.

Rollback selects one prior release for the whole run. A current build, callback
continuation or retained Save cannot switch evaluators or numerical policy.
Do not recover a failed new run by mixing old computations into it. Diagnose the
failure, then replay a fresh whole run under the prior locked release.

Saved files remain files produced by their recorded release. Installing an old
package does not change them or rewrite YAML. Preserve output/logs and provenance
from both runs; publish a replay to a separately selected destination when needed.
Save retains already computed bytes and does not reread inputs or rerun callbacks.
Publication failure can leave earlier producer files in place; rollback does not
undo those successful writes. The caller manages cleanup or a new destination
explicitly. Replay requires the prior input/resource snapshots and appropriate
platform environment, not specification text alone.

## Removal order and stop conditions

Before the first deletion, classify every import and test consumer against
release-api.json. Map every unique retained assertion to public or typed-native
coverage. Require #2002's installed validator delegation, independent packaged
schema services for style/ODM readers, and the complete ratchet. Each PR removes
its dead call sites and family-specific reference tests together; a still-live
cross-family consumer stops that deletion until its replacement is qualified.

| Step | Scope, CI and documentation updates |
| --- | --- |
| 1: public consumers | Complete #1992 and retained helper/style ports. Remove legacy public yamaa_domain/DomainRun/load_specification/generate_study_document and root-discovery facade usage. Update release-api.json and all examples to the qualified domain/check/define API. Keep an explicit migration guide for old signatures. |
| 2: assisted qualification | After the public ratchet passes the full matrix, retire the assisted required/known-gaps/missing-routes inventory and its CI consumers. Preserve history in Git. Remove no unique assertion before its public/typed replacement passes. This precedes deleting the probe endpoints. |
| 3: probes and JSON callbacks | Complete #1988/#1990: remove inheritance/1, function/1, dataset/1 and other obsolete component exports/transports. Port their consumers first. Keep JSON-text issue context and the public installed suites. Update native staging/catalogs and run release/dependency guards. |
| 4: reference application | Remove _reference_domain.py, application.py, old submission rendering, io/project.py, authoring/resolved/provenance extensions and reference-assisted adapters. Keep private packaged schema/style and ODM-reader implementations. Update python.yml and tests to the public replacement; retain structural repository validation and grammar replay. |
| 5: evaluator families | Remove planning/runtime and their dead callers, then expressions/verification/regex and their dead callers, then old specification/schema/models and residual IO/non-reader ODM implementations. Resolve the actual import graph before each PR; group an inseparable dependency closure in that PR. Remove reference-only dependencies from pyproject.toml and regenerate uv.lock only after the final consumer disappears. |
| 6: remaining reference assets | Complete #1993's R/cdiscbuilder removal after grammar/reader decisions and replacement checks. Remove the four historical resolved outputs and reviewed obsolete path-history fixtures. Update .gitattributes, manifests, source package membership and release-api.json. |
| 7: final release evidence | Repeat all gates on combined main, refresh release-readiness.md and migration-status.md with exact audit links, and tick the umbrella gates only when their complete evidence passes. |

Never hide a failure by reducing the matrix, weakening exact comparisons,
changing a benchmark expected output, introducing a compatibility evaluator or
silently dropping a resource/failure invariant. A failed workflow, missing report,
remaining forbidden import, unported unique assertion, budget regression or
unqualified package form stops the sequence. Revert the offending removal PR or
repair it while the already qualified release stays available. Keep deletion
commits small enough to restore the preceding source state from Git.

## Communication and acceptance record

The release PR updates README.md, python/README.md, R/yamaa/README.md, the docs
site, installation guides, benchmark authoring instructions and dashboard runtime
labels consistently. Document only the new public surface, with a linked migration
section for removed signatures and the numerical compatibility note. Keep the
shared site palette and opensas runtime label conventions.

The #1973 item 11 approval names the source revision, full run/audit links,
package/version records, complete ratchet, budget decision and rollback rehearsal.
The final #1994 acceptance names the post-removal main revision and its renewed
evidence. #1999 closes only after this plan is merged and the entry-gate approval
is actually recorded. Merging this document alone supplies no release approval.
