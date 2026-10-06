# Shared YAML decoding

The experimental `yaml/1` service accepts retained source bytes and returns an
owned decoded document or explicit diagnostics. Python exposes
`yamaa_native.decode_yaml(source: bytes)` and R exposes
`yamaanative::decode_yaml(source)` for an unclassed raw vector. Neither entry
point opens files, calls a host YAML parser or grants execution authority.

The parser is `saphyr-parser` at the same pinned revision used by the existing
Python/R `yaml12` packages. Only the adapter crate depends on it. Core schema
interpretation receives an owned tree and remains independent of YAML, hosts
and filesystems.

## Values and source restrictions

Input must be ASCII. Decoded strings contain Unicode scalar values without
normalization. YAML 1.2 core scalar resolution keeps legacy Boolean-looking
words and date-looking values as text. Non-finite floats become null
immediately. Explicit tags, anchors, aliases, plain merge keys, multiple
documents, non-scalar mapping keys and duplicate decoded keys are rejected.
Quoted `"<<"` keys remain ordinary strings.

Integers retain exact canonical decimal identity, including values outside the
runtime signed-i64 range. This is source representation, not permission to use
an out-of-range runtime integer. Later schema and runtime admission decide
whether a value is allowed. Mapping equality uses the same ordered scalar-key
identity as core document admission; Boolean/integer/integral-float collisions
are detected without rounding large integers or hashing content.

This corrects a known reference-reader boundary inconsistency. The existing
Python reader turns decimal `9223372036854775808` and `9223372036854775809` into
the same binary64 value and can silently collapse a mapping containing both
keys; its overflowing hexadecimal equivalent becomes a string. The shared
decoder preserves both integer keys. The default reader is unchanged, and this
difference is explicit compatibility work rather than an expected result copied
into new goldens. Full-range i64 values remain compatible.

## Outcomes

Every response contains `protocol: "yaml/1"` and `outcome`:

- `decoded`: a `document` in the [schema/1 arena format](SCHEMA_TRANSPORT.md#decoded-documents)
  and a parallel `locations` array of original byte `offset`, one-based `line`
  and one-based `column`. Children precede their sole parent; mapping order is
  preserved. Integers use decimal strings and finite floats use exact bit strings.
- `invalid`: ordered `diagnostics` with `condition`, `spec_paths` and `context`.
  Conditions are `non_ascii_source`, `invalid_yaml` or `invalid_text`. The host
  attaches the source filename to non-ASCII diagnostics.
- `resource_limit`: `phase: "yaml_source"`, `resource` and the effective `limit`.
  A quota refusal is separate from a language error and never falls back to
  another decoder.

Transport failures, including the response byte limit, raise a host error.
Invalid source, interrupted admission and resource refusals return no partial
document. R raises transport errors after the native call returns normally.

## Unicode diagnostics and error order

The pinned parser rejects an escaped surrogate before emitting its scalar.
For diagnostic reconstruction only, the adapter repairs the scanner-identified
quoted token and reparses with a cumulative work quota. A second parse uses
different valid code points at exactly the recorded escape positions. Comparing
corresponding scalar positions recovers the original invalid code points after
YAML folding and escape processing. Existing private-use characters are never
treated as sentinels. Repaired text cannot become an admitted string.

Syntax scanning and composition restrictions precede deferred mapping checks;
mapping merge/non-scalar/duplicate checks precede Unicode diagnostics. Mapping
construction follows the reference's deferred container order. Unicode findings
retain the first surrogate per string, decoded scalar offset, and ordered key
and value paths. Syntax scanners can look ahead before emitting a tagged or
anchored node; lexical failures in that lookahead still win.

General malformed-YAML parser prose and marks are not claimed identical across
libraries. For example, an unknown escape is marked at the quoted scalar start
by the pinned parser and at the offending escape character by PyYAML. Selected
restriction, duplicate-key, Unicode-path and precedence cases have independent
expected results. Broader diagnostic compatibility remains a release gate.

## Policies

| Resource | Limit |
| --- | --- |
| Source bytes per snapshot | 8,388,608 |
| Response UTF-8 bytes | 16,777,216 |
| Scanner events | 262,144 |
| Scanner decoded text bytes | 8,388,608 |
| Cumulative parser-pass source bytes | 33,554,432 |
| Document nodes / edges | 131,072 / 262,144 |
| Document depth | 64 |
| Document text bytes | 8,388,608 |
| Numeric input digits | 4,096 |
| Cumulative diagnostic path bytes | 8,388,608 |

These are logical request policies, not process-memory, allocation-failure or
concurrent-request containment guarantees. Diagnostic-only reparsing never
relaxes the source or text rules.

## Python loading and qualification

`yamaa.adapters.native_specification` captures the decoder and schema compiler
before filesystem resolution or source reads. Its confined schema closure uses
the shared decoder and has an aggregate 8 MiB source budget. The captured reader
travels with the schema bundle into specification and inherited-parent reads.
Replacing a module entry point after capture does not alter those reads. A
retained null inheritance snapshot is validated without rereading the file.

Python retains filesystem authority, parent traversal, dependency discovery and
model validation. Normalized layers use [shared composition](LAYER_COMPOSITION.md),
and named windows use the
[shared expansion service](WINDOW_EXPANSION.md). Its integer-string conversion
policy is honored explicitly: a value that exceeds the current interpreter
limit raises `NativeSchemaLimitError` with resource `host_integer_digits`.
The adapter does not change interpreter-wide limits. R's raw service retains
integer text without R numeric conversion. The existing filesystem check/read
model is not a race-free authority boundary against concurrent writers.

Eighteen independently authored complete wire cases replay in Rust and both
installed hosts. Scanner tests cover all 2,048 surrogate code points in both
escape widths, folding, continuations, private-use collisions and resource
refusals. Installed loader tests disable host YAML decoding while reading the
schema closure and every inherited source, preserve committed resolved documents
and exact execution CSVs, and check capture before IO and explicit policy errors.
Supplemental local comparison of all 514 current YAML files under `yaml/` and
`benchmarks/` found exact decoded-tree agreement with the reference reader;
this does not replace independent truth or qualify arbitrary malformed YAML.

Python remains the default and `execution_supported` remains false. Shared
workflow planning, current-schema R workflows, numerical policy and the full
release gates in issue #1585 remain open.
