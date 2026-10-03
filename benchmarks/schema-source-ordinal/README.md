# Source record ordinal

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-source-ordinal.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** assign dense sequence numbers within each group in original source
record order.

**Input:** `spec.yaml` reads interleaved groups, labels collected in reverse
alphabetical order, excluded records, and otherwise identical records.

**Variables:**

- `GROUP_ID`: the collected group identifier.
- `ROW_SEQUENCE`: consecutive sequence numbers starting at 1 within each group.
- `RECORD_LABEL`: the collected label, without alphabetical sorting.
- `SOURCE_ORDINAL`: the original one-based position of each data record.

**Note:** sequences restart at 1 in each group. Filtering removes excluded
records without changing the surviving source ordinals. Identical records
retain distinct positions and sequence values. This listing includes
`SOURCE_ORDINAL` for review.
