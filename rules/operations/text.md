---
id: operations/text
title: Text operations
status: normative
---

# Text operations

## Requirements

### Type behavior

<a id="req-0304"></a>

**REQ-0304.** `mapping` requires a string source.

### Case-insensitive inline mapping

<a id="req-0709"></a>

**REQ-0709.** `mapping.case_sensitive: true` compares its source with
dictionary keys by the exact equality above.

<a id="req-0710"></a>

**REQ-0710.** When `case_sensitive` is `false`, fold the source and every
dictionary key by replacing `U+0061` through `U+007A` with `U+0041` through
`U+005A` and leaving every other scalar unchanged. Compare the folded
sequences by exact equality. Dictionary keys must be unique after this
fold; otherwise validation fails with `ambiguous_dictionary` and reports
the folded key and original entries. A non-ASCII value can match only the
same non-ASCII scalar sequence, apart from ASCII letters that also occur
in that sequence.

### Interface behavior

<a id="req-1110"></a>

**REQ-1110.** The `expressions.mapping` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `expressions.mapping.source` | String variable used as the dictionary key, or a [filtered source](lookup.md#req-1052) carrying it with a filter, a selection, or an absent value. |
| `expressions.mapping.dict` | Source-value to result-value dictionary, written inline or as the path of a YAML file holding it. A path is read once during workflow planning through the spec's [project resources](../storage/resources.md); its content must satisfy the inline dictionary contract. |
| `expressions.mapping.case_sensitive` | Compare exactly when true; use [Values](../values.md) ASCII folding when false. |
| `expressions.mapping.missing` | Value returned when the source is missing; without it, a missing source fails ([REQ-0344](../execution/handlers.md#req-0344)). |
| `expressions.mapping.unmapped` | Value returned when the source is present but has no dictionary entry; without it, such a source fails. `missing` never answers it. |
| `Result` | Looks up a string source in a dictionary, given inline or loaded from a YAML file. Case-insensitive lookup folds ASCII a-z to A-Z under [Values](../values.md); folded keys must be unique. |

## Error conditions

### Errors

<a id="req-0714"></a>

**REQ-0714.** Dictionary keys that collide after the ASCII fold: fail
validation with `ambiguous_dictionary`.
