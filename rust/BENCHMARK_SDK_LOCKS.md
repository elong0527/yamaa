# yamaa benchmark SDK lock coherence

The five R function studies retain existing hash-free `renv.lock` files. Their
`Packages.yamaa.Version` pins the prepared R package version; installed activation
checks the SDK and all called program packages before binding or study data.
Python studies reuse the existing `python/uv.lock`, whose SDK version follows
the prepared Python package. Program package versions remain independently pinned.

Preparing SDK version 0.3.0 while the R studies still pinned 0.2.0 exposed a
real admission failure: fresh strict R checks failed in `producer_builds.R`.
The retained complete report identified REQ-0697 `runtime_artifact_mismatch`,
expected 0.2.0 and installed 0.3.0. Two fresh builds entered lock verification
only, with no binding, invocation, node entry or study read. This refusal is
correct; accepting the mismatch would weaken the qualified activation boundary.

PR #1904 corrected the SDK entries in those five existing locks. Issue #1907
adds the automatic coherence guard and installed mismatch regression. No program
version, lock structure, digest, input, conformance vector or expected output is
changed. `tools/check_benchmark_sdk_versions.py` compares the prepared Python/R
versions and every benchmark R SDK entry directly. It reads metadata only, writes
nothing, and runs before native Rust and package builds. Five independent guard
cases cover coherent immutable files, the original mismatch, missing or malformed
SDK entries, host-version disagreement and original JSON failures.

The installed R producer witness separately constructs a valid SDK lock for
version 99.0.0 and removes the study input after preparation. The actual installed
verifier must retain the complete REQ-0697 refusal after one lock call, before any
resolver, node or source read. Ordinary successful builds still pin the exact
independent rounded producer and consumer bytes, repeated activation and original
failure/interrupt identity. The mismatch case supplies no accepted artifact.

Full current-source qualification, installed forms, complete review, every
applicable exact-head check, artifact audits and a fresh protected guard are
required before this correction closes. The separate #1897 metadata-origin
correction was merged through GitHub in PR #1901 while its fresh strict R check
had failed on the preceding base. PR #1904 fixed the locks before #1901 merged;
this correction freshly qualifies the resulting complete source. No source-system-version override or publication decision is
inferred; #1887/#1866/#1857 and parent acceptance/release gates stay open.
