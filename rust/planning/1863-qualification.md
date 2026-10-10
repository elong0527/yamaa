# #1863 complete producer graph metadata qualification

This slice proves complete static graph admission and function-slot projection.
It keeps all compiled plans sealed and explicitly denies execution. It closes
neither #1741's installed workflow acceptance nor #1585 migration or release gates.

Sixteen new independent shared tests cover recursive graphs, diamonds, repeated
aliases, authored link and producer-first order, local/global slots, unique shared
function selection, a moved environment's unchanged case ownership, complete
closure refusal, cyclic routes, invalid producer/consumer calls, cross-branch
metadata contradictions and aggregate quotas, including callable text absent from
specification trees and retained signature parameters. Adapter tests retain original
YAML/schema/parent bytes and inherited output origins, compare equal snapshots
directly and reject conflicting parent bytes found only across separate branches.
The complete diagnostic registry reaches the new REQ-0534 cycle cause through an
actual graph rejection. Existing #1858 admission and refusal witnesses also pass.

Installed public Python/R witnesses additionally retain the ordinary producer
refusal: static check/build report `unsupported_operation` at `input.P.schema`,
return no accepted table and deny explicit save, with both prospective artifacts
absent. This is a refusal regression, not installed workflow qualification.

[PR #1865](https://github.com/elong0527/yamaa/pull/1865) merged on 2026-10-10
as `4bfa2d8300539b598f71cb26991914301cd088a1`, closing #1863. Its source tree
equals qualified head `0d13160446a72a95a8924d1cc9692f05d44a84ba` directly.
All 23 exact-head checks, all 16 native jobs, complete assessed review and all
40 downloaded artifact audits passed before a fresh guarded merge. The valid
review finding was fixed: execution refusal now reports each authored root
producer schema path in order, and a single external-input node retains a
nonempty denial at its admitted input field.

Actual Linux/macOS logs pass 1,037 tests in each debug/release-test mode;
Windows passes 1,028 in each mode with Unix-only cases excluded. Each mode has
125 targets. Strict Clippy for all five crates, dependency/requirement guards and
all 48 tooling tests pass. Fourteen clean package audits compare retained authored
source and fixture bytes against the final head without hashes or cache members.
All six installed Python forms pass 24 supplemental suites and 17 public environment
methods each; both exercised R forms pass 18 supplemental suites and strict
18-script `Status: OK` checks. PortableLibmV1's 30,033 exact observations match
across six hosts/interpreter versions. See [the final evidence](https://github.com/elong0527/yamaa/pull/1865#issuecomment-6096372081)
and [native run](https://github.com/elong0527/yamaa/actions/runs/38042513913).

The 954-row inventory remains reference-assisted with five broader native gaps.
The public cohort retains 96 passing tuples and 12 Windows R tuples explicitly
unqualified. Component qualification does not close those migration/release gates.

Original study truth, PortableLibmV1, existing UV/hash-free R locks, approved
resource roots and old Define-XML remain unchanged. #1867 isolates native graph
capture. #1741 still owns activation before data, producer-once execution, rounded serialized
ingestion, retained failures and explicit publication. Windows R scope remains
the unanswered single-decision issue #1857.
