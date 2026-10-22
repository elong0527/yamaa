# Reject a Randomization Time With a Fraction of a Second

**Goal:** carry the date and time of randomization (`RANDDTM`) onto a
subject listing.

**Input:** a Parquet export from the randomization system with one record
per subject, holding the moment of randomization as a date and time.

**Variables:**

- `RANDDTM` would be the date and time the subject was randomized, as the
  export records it.

A study date and time is recorded to the whole second, but the second
subject's time, 10:40:12.250 on 2 March 2026, carries a quarter of a second
as well. Keeping 10:40:12 or moving to 10:40:13 would each change a recorded
value, and the study states no such rule, so the run is rejected while the
export is read, and no artifact is accepted.

## How to fix

Ask the randomization system to export its times to the whole second, so
that the moment it reports is the one the study keeps:

```yaml
input:
  RND: input/rand.parquet
```

If the study needs the fraction of a second, export the time as text and
state in the study how that text is read.
