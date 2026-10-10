# #1858 producer compiler prerequisite qualification

This change qualifies bounded contract and compiler admission. It does not close
#1741, promote a benchmark to shared workflow execution, or authorize release.

The local source passes 1,012 shared Rust tests in debug and 1,012 in the optimized
`release-test` profile, each across 120 targets including three documentation-test
targets. The 25 new independent tests cover output selection/type/label contracts,
consumer declarations and origins, metadata contradictions/quotas, portable
REQ-0534/REQ-0523/REQ-0535 causes, and denied engine execution authority. The complete
diagnostic registry reaches every registered cause with independent literal
vocabulary. Strict Clippy passes for all five crates, as do dependency-direction
and requirement guards and all 48 Rust tooling tests.

The initial admission packages passed all 24 supplemental suites in each fresh
Python wheel/source installation (including all 13 public environment methods),
all 18 R supplemental suites and all 18 strict R source-check scripts with
`Status: OK`. All three package source/member audits pass by direct byte equality.
The broad installed Python regression passes 4,814 tests with 94 skips; one POSIX
socket test was rerun with its required local socket permission.

After pre-merge review, repeated producer identities now reject contradictory
contracts or original snapshots, and undeclared/schema-less adapter metadata
retains the portable REQ-0534 cause. Explicit-null schema retains its earlier
REQ-0287 structural refusal. Corrected package forms are being qualified again.
PR #1859 platform jobs, completed review and final-head artifact auditing remain
required before merge; local component results do not substitute for them.

The sealed metadata result retains original consumer/producer/parent documents
and separate schema-link, consumer-artifact and producer-output layer origins.
The adapter compares the same captured schema closure directly by module identity
and bytes. Resolved location facts remain metadata with no IO authority. Ordinary
public checking and building still refuse producer `schema` declarations.

No expected study artifact, producer fixture, numerical policy, content digest or
Define-XML implementation changes. #1741 must qualify native graph/resource
preparation, recursive dependency validation, producer-once execution, rounded
serialization before consumer ingestion, retained failures, activation before
study reads, and explicit successful save through both installed public hosts.
