# Infusion duration as ISO 8601 duration text

**Goal:** write each administration's collected duration as `EXDUR`.

**Input:** collected exposure records, one per administration (`EXSEQ`),
with the treatment (`EXTRT`), the start date (`EXSTDTC`) and, where the
case report form collected it, the duration in whole minutes.

**Variables:**

- `EXDUR` is the collected duration as ISO 8601 duration text: `P`, then
  the days, then `T` and the hours and minutes, each part that is zero
  left out, so 120 minutes is `PT2H`, 90 minutes `PT1H30M` and 45 minutes
  `PT45M`. A day is the largest part, so a 46-hour continuous infusion is
  `P1DT22H`, never `PT46H`. A bolus injection recorded as zero minutes is
  `PT0S`. Empty when the duration was not collected; it is never computed
  from the start date.
