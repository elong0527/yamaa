# Windows R and locked-build release evidence

This ledger assesses main `da02b59b386cfd22a20a7ae062e602c92f6f8983` for
[#1996](https://github.com/elong0527/yamaa/issues/1996). The repair and complete
premerge evidence belong to [#2129](https://github.com/elong0527/yamaa/pull/2129).
Combined-main acceptance remains pending. No issue, release or transition gate
is approved by this ledger.

## Retained main observations

[Main run 38481589341](https://github.com/elong0527/yamaa/actions/runs/38481589341)
tested `8f7a5e78d9e1ff401881f33f046e27d057043524`, before the coordinated
0.4.0 removal/version composition. Its eighteen installed Python lanes, all
three R hosts and complete public/producer cohort job report success. It is a
failed run: all three shared Rust jobs fail the original public BMI numerical
witness, and the exact math comparison is skipped. The audit also crashes on a
missing independent `polars` reader, so it supplies no completed qualification
record. Those failures remain release blockers; installed job success alone does
not complete this run.

The [Windows R supplemental artifact
11857557933](https://github.com/elong0527/yamaa/actions/runs/38481589341/artifacts/11857557933)
was downloaded and inspected locally. It records R 4.6.1, package 0.3.0, embedded
core 0.1.0, the exact source above and `yamaa_0.3.0.tar.gz`. All twelve declared
supplemental suites pass, including public environment/functions, producers,
verification and producer logs, Define, lifetime/interrupt/reentrancy contract,
grammar and file resources. Its [strict package-check artifact
11857588056](https://github.com/elong0527/yamaa/actions/runs/38481589341/artifacts/11857588056)
was separately downloaded and contains an exact `Status: OK` line. This evidence
retains its original package/source identities and does not qualify 0.4.0.

[Main run 38486214032](https://github.com/elong0527/yamaa/actions/runs/38486214032)
names `da02b59b386cfd22a20a7ae062e602c92f6f8983`, but is cancelled with no
jobs. It cannot supply the final source matrix or audit.

## Repairs and branch qualification

The shared Rust numerical suite uses the locked `benchmark` dependency group.
The original BMI fixture declares `yamaa_benchmarks.bmi`; without that installed
distribution, the public engine correctly refuses runtime admission with
`runtime_artifact_mismatch`/REQ-0697. Installing the reviewed benchmark group
restores the actual public run without weakening admission or changing fixture
truth. Local reproduction removes only that distribution from an isolated
exact-main package environment, observes the original failure, then reinstalls
the locked group and passes all fourteen frozen numerical/public witness tests.

The evidence audit installs its independent Polars/PyArrow/YAML/Pydantic readers
from the canonical uv lock without installing the candidate yamaa runtime. A
missing reader is retained as explicit missing evidence with `complete=false`,
so an unavailable dependency never becomes a pass or loses the written record.

Pushes to explicit `qualification/issue-*` branches run all eighteen Python
wheel/source lanes, three R source hosts and strict checks, three shared Rust
checks, nine numerical assessments, exact math comparison, complete
public/producer cohorts and the required evidence audit. The branch record says
`qualification_scope: reviewed-branch`; main says `combined-main`. Ordinary PR
sampling and manual dispatch coverage retain their existing scope. Superseded
branch runs may cancel; active main runs finish.

The small `native-run-audit-record` artifact retains JSON/Markdown verdicts and
run/source/job/artifact metadata for review. The full `native-run-audit` retains
the canonical source files and original artifact bytes needed for independent
replay. Package/source/lock checks compare actual bytes; no content identity
digest is introduced. Refer to #2129 for the final immutable branch revision,
run, artifact and independent replay results.

## Closure evidence still required

After merge, one successful full main run must retain the actual 0.4.0
wheel/source packages and R source archive, canonical lock and workspace members
including `build.rs`, transitive graph/features and toolchains, all installed
supplemental routes, twelve Windows R public-cohort cases, the complete 316-case
ratchet, strict `Status: OK` on all three R hosts and a complete independently
replayed audit. The exact declaring-file file-resource/publication and public
lifetime/interrupt suites must belong to that same source/package matrix.

The former assisted `required.json` and `missing-routes.json` were retired with
their obsolete consumers in `384cb9111c2367aaf12298f29afbec3fc05831e2`.
Current [public qualification data](../qualification/public-conformance.json)
names Python and R passing sets and retains all three R host lanes, including
Windows; the audit independently enumerates every required platform/form.
Route retirement is recorded in [the qualification guide](../qualification/README.md).
This does not waive a current public route or a failed/missing main result.
