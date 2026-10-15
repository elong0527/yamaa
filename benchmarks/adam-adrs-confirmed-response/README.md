# Confirmed Response

**Goal:** derive the confirmation flag `CONFIRMED` for tumor
response assessments.

**Input:** response assessments within a subject, each with a
sequence number `RSSEQ`, a collection date `RSDTC`, and a
standardized result `RSSTRESC`.

**Variables:**

- `CONFIRMED`: `Y` when the analysis result `AVALC` is progressive
  disease (`PD`), which needs no confirmation, or when a partial
  (`PR`) or complete (`CR`) response is followed at least 28 days
  later by another partial or complete response; `N` when the current
  assessment has no result, or when the next response is too early,
  is not a response, or does not exist. Each assessment is compared
  with the next one in analysis date `ADT` order within a subject.

**Note:** assessments sharing a date are ordered by sequence
number `RSSEQ`: the higher-numbered one is the next assessment for
the lower-numbered one, zero days later, so it never confirms it.
