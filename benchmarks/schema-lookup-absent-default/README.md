# First Dose and Death Dates Stay Blank Without a Record

**Goal:** derive each subject's date of first exposure to treatment
(`TRTSDT`) and date of death (`DTHDT`), each left blank when the subject has
no record to read it from.

**Input:** one `spec.yaml` over demographics, exposure records carrying each
administration's start date and dose, and disposition records carrying the
disposition event and its date.

**Variables:**

- `TRTSDT` is the start date of the subject's earliest administration with a
  dose above zero; a subject whose administrations all have a zero dose, or
  who has none, has no first exposure date.
- `DTHDT` is the date of the subject's death disposition record; a subject
  with no death record has no date of death.

**Note:** a subject without a record to read is kept, with the date blank,
instead of stopping the run, and each such blank is counted so the run
reports how often it happened.
