# Intermediate Derived Names Resolve Everywhere

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-intermediate-derived-resolution.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the #767 resolution rule: an intermediate's derived
names resolve everywhere the intermediate's own donor records are in scope.

> **Engine coverage:** the Python engine implements this behavior.
> The R engine does not implement it yet.

**Input:** exposure records with sequence numbers (`EXSEQ`), treatment
names (`EXTRT`), and ongoing flags (`EXONGO`) in mixed case.

**Variables:**

- `STUDYID`: the study identifier, carried through.
- `USUBJID`: the unique subject identifier, carried through.
- `PARAM`: the parameter name, a literal.
- `AVAL`: the sequence number of the last non-ongoing exposure record.
- `AVALC`: the uppercased treatment name of that record.

**Mechanism:** the `LASTEX` intermediate derives `ONGO_U` and `TRT_U`
from the exposure records, filters to the non-ongoing records with
`EX.ONGO_U = 'N'`, orders by the derived `EX.TRT_U` descending, and keeps
the first record per subject. `ONGO_U` and `TRT_U` appear in `filter:`,
`order_by:`, and `columns:`; the output reads `LASTEX.EXSEQ` (stored) and
`LASTEX.TRT_U` (derived). The derived read is valid because the
intermediate declares `keep`, which makes the computed value a row-scoped
read.

**Standard:** ADaM | **Domain:** ADEX
