# Baseline Among Same-Day Results

**Goal:** flag each subject's baseline result (`ABLFL`) when several
results share the latest date on or before treatment start.

**Input:** one `spec.yaml` over laboratory results carrying subject,
sequence number (`LBSEQ`), collection date (`ADT`) and time (`ATM`),
result (`AVAL`), and the subject's treatment start date (`TRTSDT`), all
carried through unchanged.

**Variables:**

- `ABLFL` is `Y` on the subject's baseline result and blank on every
  other result. The baseline has the latest date on or before treatment
  start; among the results on that date the one collected last is
  chosen, and results collected at the same time are separated by the
  higher sequence number.

**Note:** a result with no collection time counts as earlier than every
timed result on the same date, so a timed result is chosen over it even
when its sequence number is lower.
