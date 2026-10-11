# Scalar padding

The core `text_pad::StringPad` service and native declarative compiler implement
`str_pad` for named operands in columns, row substitutions and selected nested
column cases. `width` is a positive mathematical integer. Present scalars use
shared canonical string conversion, count Unicode scalar values, and receive
ASCII spaces on the left. Longer values retain their complete text. Missing
values pass through without a handler or output allocation.

```yaml
- name: RESULT
  type: str
  derivation:
    str_pad: {source: SRC.TEXT, width: 4}
```

The core preserves full signed i64 values, signed binary64 zero, canonical date
and datetime text, UTF-8 and NUL. It never performs the assignment's final output
conversion. The engine reads the operand once and applies that existing outer
conversion once; conversion failure retains the full padded value and completed
row keys. Opaque source failures retain their original allocation and identity,
without retry or recovery.

Before ownership, the compiler scans every occurrence in columns, rows and all
nested cases. Checked metadata accounting (eight times source/path/width bytes
plus 512 per occurrence) has a 16 MiB ceiling, independent of configurable model
limits. Constructor admission uses the same bound before width scanning and
copies. Canonical primitive formatting is bounded by 327 bytes; authored text
stays borrowed inside the scalar service. The engine admits operand copies,
subject scan work and the complete rendered UTF-8 byte count through the run's
cumulative counters before rendering. An unrepresentable positive width remains
a valid declaration: present output refuses its size; missing still passes
through. Resource refusal cannot create a semantic width failure or a handler.

Nonpositive width has a core-owned validation/invalid_field_type/REQ-1261 finding
with the complete authored width path and expected-positive-integer context.
Noninteger authored width retains existing schema REQ-0287 classification.
Within a case derivation that contains an invalid width, declaration findings
(grammar, numeric declarations and widths) retain authored order before reference
binding. Existing consumers without such a width use their existing checkpoint.

`text_pad_reference.json` retains independent legacy captures obtained before
candidate qualification: complete specification/input text, output CSV, findings
and available empty handler ledgers. Native tests and installed Python/R tests
compare their complete outputs and reached shared findings. The unchanged
`negative-str-pad-width` fixture matches its full original golden before input
capture. Native schema reports still omit the legacy loader's source/entry facts;
the complete noninteger capture retains that gap explicitly. Existing case
predicate unknown-field requirement differences remain separate diagnostic debt.

The original positive `adam-adlb-end-of-treatment` and
`adam-adlb-supplb-padded-key` fixtures still encounter intermediate expression
keys and broader row/multiple-source gates. Their authored specifications, inputs
and expected bytes remain unchanged. This scalar slice does not qualify those
complete fixtures or close the full compiler/dataset migration gate.
