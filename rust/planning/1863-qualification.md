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

Before a ready PR, qualify the final source in debug and optimized Rust tests,
strict Clippy for all five crates, dependency/requirement guards and tooling tests;
build clean wheel/source/R packages, directly audit source members and run both
installed Python forms and strict R checks/supplements. Every applicable exact-head
CI check, complete review, installed artifact audit and fresh merge guard remains
required before merge. The PR records actual completed results.

Original study truth, PortableLibmV1, existing UV/hash-free R locks, approved
resource roots and old Define-XML remain unchanged. #1741 still owns native graph
resolution, activation before data, producer-once execution, rounded serialized
ingestion, retained failures and explicit publication. Windows R scope remains
the unanswered single-decision issue #1857.
