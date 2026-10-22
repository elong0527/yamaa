# Aggregate Bindings Read an Intermediate

**Goal:** demonstrate an aggregate's per-record bindings for #789: a
binding reads a named intermediate that declares `keep`, so one
row-scoped value is visible to every record the aggregate reduces.

**Input:** exposure records with sequence numbers (`EXSEQ`) and
doses (`EXDOSE`), plus dose caps (`CAPDOSE`), which a subject may
have more than one of.

**Variables:**

- `CUMDOSE`: the cumulative dose over all of the subject's exposure
  records, after capping each record's dose at the subject's lowest
  cap. A subject with no cap has their doses summed uncapped, and a
  record with a missing dose counts as the full cap.

**Mechanism:** the `DOSECAP` intermediate sorts each subject's cap
records by `CAPDOSE` and keeps the first, the lowest cap, so it
holds one record per subject. The aggregate binds `CAP` from
`DOSECAP.CAPDOSE` for each exposure record, caps that record's
`DOSE` with `least`, which ignores a missing value, and the reducer
sums the capped values. The `keep` declaration is what makes the
intermediate read valid. Each output row is built from the
subject's first exposure record (`EXSEQ` 1), one row per subject.
