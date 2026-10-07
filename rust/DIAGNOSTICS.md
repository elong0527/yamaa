# Portable diagnostic foundation

The first #1753 slice moves numeric evaluation, arithmetic/reduction and
completed-result conversion diagnostics into `yamaa-core::diagnostic`. The
original-specification runner and existing dataset/numeric transports consume
those core diagnostics. Adapters encode runtime values and source offsets but
do not reconstruct these families' semantic context or choose their requirements.

`Diagnostic` owns its typed context, ordered specification paths, optional UTF-8
source span and optional operand route. Diagnostic integers outside runtime i64
remain canonical decimal text with a distinct type. Absent source geometry stays
absent. Original opaque resolver errors have no normative projection: the caller
retains the error and its identity rather than manufacturing `unknown_field`.

## Registry identity and compatibility

Each `ConditionCode` names one semantic cause and maps to one phase, public
condition and requirement in the registry. A public condition string is not a
unique registry key in the existing language:

| Semantic cause | Public condition | Requirement |
| --- | --- | --- |
| Invalid conversion input/text | `conversion_failed` | REQ-0013 |
| Nonintegral or out-of-range integer conversion | `conversion_failed` | REQ-0021 |
| Invalid temporal conversion text | `conversion_failed` | REQ-0601 |
| Nonnumeric compute input | `incompatible_input_type` | REQ-0444 |
| Floating-point decimal-rounding digits | `incompatible_input_type` | REQ-0418 |

Collapsing these mappings would change independent expected diagnostics. Internal
cause identities preserve the distinctions without adding a new wire field or
changing public condition names. The existing `phase`, `condition` and
`requirement` methods delegate to the same registry as the diagnostic projection.
Registry identity has no serialization or content-hash role.

## Evidence and remaining work

The core integration test reaches every registered cause through real evaluation
or conversion and compares independently stated public vocabulary. It also checks
nested source location, literal/arithmetic integer overflow, a 5,000-digit failed
conversion and an opaque non-Clone host error whose identity and first-failure
order survive diagnostic inspection. Adding a registry entry without a reached
test fails that coverage assertion. The unchanged shared transport/lifecycle and
original-document fixtures continue to pin exact complete outcomes.

`tools/check_diagnostics.py` checks literal requirement IDs across every Rust
crate's source against current normative rule definitions, including mappings
outside the migrated registry. Native CI runs this guard and its mutation tests.
The existing canonical rules validator separately checks definition uniqueness
and historical IDs. The guard does not infer requirements from documentation
citations or count component tests as complete-run qualification.

This is the first exercised family, not completion of #1753. Parser/schema,
binding, predicate, function, verification and ingestion diagnostics still need
conversion; some remain constructed in adapters. Inherited source/entry/parent
provenance, richer structured context and unified serialization across all
protocols must migrate with those families. Existing per-protocol wire wrappers
remain temporarily for unmigrated errors. No registry completeness, public API
cutover, new numerical policy or benchmark inventory promotion is claimed.
