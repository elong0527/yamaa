# Baseline Skips Missing Results

**Goal:** flag each subject's baseline result (`ABLFL`) as the latest
result with a value on or before treatment start.

**Input:** one `spec.yaml` over laboratory records carrying subject,
sequence number (`LBSEQ`), collection date (`ADT`), result (`AVAL`),
which may be missing, and the subject's treatment start date
(`TRTSDT`), all carried through unchanged.

**Variables:**

- `ABLFL` is `Y` on the latest record on or before treatment start that
  has a result, and blank on every other record. A record whose result
  is missing is passed over even when it is the latest before treatment
  starts, so a subject whose only record before then has no result has
  no baseline.
