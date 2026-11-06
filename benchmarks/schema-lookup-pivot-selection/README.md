# Choose the Last Height and the Last Weight of Each Subject

**Goal:** one row per subject carrying the last measured height (`HEIGHT`)
and the last measured weight (`WEIGHT`).

**Input:** one `spec.yaml` over demographics records and vital signs
records, each naming its test, its date and its numeric result.

**Variables:**

- `HEIGHT` is the result of the subject's latest height measurement; empty
  for a subject with none.
- `WEIGHT` is the result of the subject's latest weight measurement, the
  later record of two on one date; empty for a subject with none.

**Note:** one declaration names the two tests, and the choice of the latest
record is made for each test alone, so a subject's later weight never
displaces an earlier height. Measurements of other tests take no part.
