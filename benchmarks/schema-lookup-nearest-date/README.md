# Take Each Subject's Weight Nearest the Randomization Date

**Goal:** give each subject the weight measured nearest the date of
randomization (`WTR`) and the date of that measurement (`WTRDT`).

**Input:** one `spec.yaml` over one record per subject, carrying the date of
randomization, and weight records, each with its sequence number,
measurement date and result in kilograms.

**Variables:**

- `RANDDT`: the date of randomization.
- `WTRDT`: the date of the subject's weight measurement nearest the date of
  randomization, before or after it, counted in calendar days. Of two
  measurements as many days away, the earlier is chosen.
- `WTR`: the weight measured on that date, in kilograms.

**Note:** days are counted on the calendar, so a measurement on 27 February
2024 is three days before 1 March 2024, the leap day included, and one on
3 March, two days after, is nearer.
