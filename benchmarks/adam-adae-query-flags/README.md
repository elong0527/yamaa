# Query Membership Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-query-flags.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** group each adverse event by safety topic, showing which
coded term falls under two Standardized MedDRA Query (SMQ) groupings
and one sponsor-defined customized query (CQ) grouping.

**Input:** one row per collected adverse event with the reported term
`AETERM` and the Medical Dictionary for Regulatory Activities
(MedDRA) coded term `AEDECOD`. A query dictionary lists each coded
term under its query groupings.

**Variables:**

The first and second groupings use parallel columns; each query
always fills the same place.

- `SMQ01NAM`: name of the first standardized query grouping the
  coded term belongs to; empty when the term is not in it.
- `SMQ01CD`: dictionary code of the first standardized query
  grouping; empty when the term is not in it.
- `SMQ01SC`: whether membership is broad or narrow scope
  (BROAD/NARROW); empty when the term is not in it.
- `SMQ02NAM`: name of the second standardized query grouping the
  coded term belongs to; empty when the term is not in it.
- `SMQ02CD`: dictionary code of the second standardized query
  grouping; empty when the term is not in it.
- `SMQ02SC`: whether membership is broad or narrow scope
  (BROAD/NARROW); empty when the term is not in it.
- `CQ01NAM`: name of the sponsor-defined grouping the term belongs
  to; empty when not listed. No code or scope applies.

**Note:** an event can sit in a standardized grouping and in the
sponsor grouping at the same time, each shown in its own place:
Stevens-Johnson syndrome carries both the skin-reaction grouping and
the sponsor grouping. Two events with the same coded term always show
the same grouping entries: both nausea events carry the sponsor
grouping. An event still awaiting coding, with no `AEDECOD`, belongs
to no grouping.

**Standard:** ADaM | **Domain:** ADAE
