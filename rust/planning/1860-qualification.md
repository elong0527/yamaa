# #1860 CSV precision qualification

This change implements shared CSV precision admission, diagnostics and artifact
serialization. It closes neither #1741 workflow acceptance nor #1585 migration
and does not authorize release cutover.

Independent core and adapter tests cover genuine and adjacent binary64 ties,
`2.675`, unsigned rounded zero, subnormals, fixed trailing zeros, 5,000-place
spelling, integer metadata quotas, byte ceilings and refusal before cell authority.
Whole shared results retain unrounded derived values and exact serialized bytes
across failed publication and repeated explicit save after preparation is dropped.
Negative/Parquet declarations retain REQ-0744/REQ-0762 validation causes; an earlier
division-by-zero failure remains original and publication is denied. The complete
diagnostic registry reaches both new causes with independent literal vocabulary.

Fresh installed public Python qualification passes all 16 methods. The precision
witness compares original tables, dependent calculations, unrounded verification
observations and exact saved bytes; it checks 5,000-place output, arbitrary-width
quota refusal, structural invalid declarations and failure/save gates. Existing
opaque host-failure and original-interrupt witnesses also carry declared precision.
The fresh R source check passes all 18 scripts with strict `Status: OK`, including
the equivalent public precision witness.

The shared source passes 1,021 tests in debug and 1,021 in the optimized
`release-test` profile, each across 122 targets, including three documentation-test
targets. Its nine new shared tests cover the core formatter, CSV codec and whole
result boundaries. Strict Clippy passes all five crates; dependency/requirement
guards and all 48 Rust tooling tests pass. Fresh final package-form supplements
and direct source/member audits are required before the PR is ready.
Local evidence does not substitute for the final-head CI matrix. Every applicable
check, completed full review, installed platform evidence and fresh merge guard
must pass before merge; the final PR records their actual results.

No committed expected study artifact changes. The source fixture values
`1.234/2.345 -> CSV 1.23/2.35` remain independent truth; the two-reference
`2.46/4.70` consumer witness remains #1741. There is no POWER dependency, content
digest, new repository lock, Define-XML retirement or resource-path policy change.
