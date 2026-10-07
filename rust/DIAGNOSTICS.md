# Portable diagnostic foundation

The #1753 slices move numeric evaluation, arithmetic/reduction, completed-result
conversion, classified resource, original-document preflight, output-declaration
and numeric/aggregate grammar diagnostics into
`yamaa-core::diagnostic`. The
original-specification runner and existing dataset/numeric transports consume
those core diagnostics. Adapters encode runtime values and source offsets but
do not reconstruct these families' semantic context or choose their requirements.

`Diagnostic` owns scalar and ordered sequence context, ordered specification paths, optional UTF-8
source span and optional operand route. Diagnostic integers outside runtime i64
remain canonical decimal text with a distinct type. Absent source geometry stays
absent. Original opaque resolver errors have no normative projection: the caller
retains the error and its identity rather than manufacturing `unknown_field`.

## Registry identity and compatibility

Each `ConditionCode` names one semantic cause and maps to one phase, public
condition and optional requirement in the registry. A public condition string is not a
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

Original preflight findings for undeclared row columns and unavailable drivers
retain their existing null requirement. Assigning a requirement merely to make
the registry non-null would change independent truth. The core projection also
preserves authored row indexes, column names, ordered multi-path conflicts,
missing driver values and empty/repeated row or group lists. The report adapter
represents these typed findings as the existing JSON shape. Resource failures
use that same projection without a tagged-scalar encode/decode round trip.

## Evidence and remaining work

The core integration test reaches every registered cause through evaluation,
conversion, resource classification or actual original-document compilation and
compares independently stated public vocabulary and preflight context. It also checks
nested source location, literal/arithmetic integer overflow, a 5,000-digit failed
conversion and an opaque non-Clone host error whose identity and first-failure
order survive diagnostic inspection. Adding a registry entry without a reached
test fails that coverage assertion. The unchanged shared transport/lifecycle and
original-document fixtures continue to pin exact complete outcomes.

Five authored preflight documents carry sixteen complete ordered findings from
the independent reference implementation in `tests/fixtures/preflight.tsv`. Rust
and both installed hosts compare those complete envelopes before any source
authority is reached. The existing installed original-document suite separately
checks the Parquet redundant-type preflight finding; the core test exercises all
five declared type spellings. These are preparation and retained-run contracts,
not public API or whole-inventory qualification.

Five independent output-declaration documents carry eight complete ordered
findings in `tests/fixtures/output_declarations.tsv`. The reference implementation
produced these complete failure reports after source ingestion and derivation.
Core output findings now own their vocabulary and geometry; the report adapter
uses the same typed projection as preflight and resource failures. Rust and both
installed hosts compare the full reports, including source tables and capture
accounting, then require repeated save attempts to fail without publication or
recapture after the compiled handle is released. Output declaration checks still
follow derivation, key checks and verification.

Seven authored grammar documents in `tests/fixtures/grammar_diagnostics.tsv`
pin complete independent failed reports after ingestion and binding. Numeric
and aggregate parser findings own their context in core and delegate existing
condition/requirement accessors to the registry. Written function case, optional
argument count, prohibited construct and nested reducer names survive projection.
They retain distinct numeric and aggregate requirement mappings even when their
public condition strings match. Inconsistent caller-supplied function spans
produce no semantic projection; parser policy limits remain separate failures.

`tools/check_diagnostics.py` checks literal requirement IDs across every Rust
crate's source against current normative rule definitions, including mappings
outside the migrated registry. Native CI runs this guard and its mutation tests.
The existing canonical rules validator separately checks definition uniqueness
and historical IDs. The guard does not infer requirements from documentation
citations or count component tests as complete-run qualification.

This does not complete #1753. Other parser/schema,
binding, predicate, function, verification and ingestion diagnostics still need
conversion; some remain constructed in adapters. Inherited source/entry/parent
provenance, additional structured context and unified serialization across all
protocols must migrate with those families. Existing per-protocol wire wrappers
remain temporarily for unmigrated errors. No registry completeness, public API
cutover, new numerical policy or benchmark inventory promotion is claimed.

Original output-window validation now projects through the common core diagnostic.
The canonical registry owns zero offset, required order and forbidden order;
`WindowFinding::definition` and its typed context use that same mapping. Three
independent complete failed reports retain authored paths, source reads, source
tables and save rejection. Window predicate and named-selection findings remain
separately represented until their next service migration.
