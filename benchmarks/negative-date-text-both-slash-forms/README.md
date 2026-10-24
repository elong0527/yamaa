# Reject Two Day and Month Orders for Slashed Start Dates

**Goal:** read the analysis start of each concomitant medication as ISO 8601
text (`ASTDTC`) from the collected start (`CMSTDAT`).

**Input:** concomitant medication records, each identified by its sequence
number (`CMSEQ`), carrying the collected start as the site wrote it, here
slashed dates such as `03/04/2001`.

**Variables:**

- `ASTDTC` would be the start as ISO 8601 text. A slashed date reads as
  month, day and year when the month comes first, and as day, month and year
  when the day comes first. Both orders are accepted at once here, so a
  slashed date has no single reading, and the run stops before any record is
  read.

**Note:** `03/04/2001` is March 4 when the month comes first and April 3
when the day comes first, so the order must be one decision for the study,
never a guess per record.

## How to fix

Keep the one order the sites used, as their case report forms or data
transfer agreement state it. For sites that write the month first:

```yaml
derivation:
  date_text:
    source: CM.CMSTDAT
    forms: [iso, mdy_slash]
```

For sites that write the day first, list `dmy_slash` instead. When sites
used different orders, correct the collected values to one order upstream
before reading them.
