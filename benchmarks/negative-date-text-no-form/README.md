# Reject a Start Date in an Unlisted Spelling

**Goal:** read the analysis start of each concomitant medication as ISO 8601
text (`ASTDTC`) and as a date (`ASTDT`) from the collected start
(`CMSTDAT`).

**Input:** concomitant medication records, each identified by its sequence
number (`CMSEQ`), carrying the collected start as the site wrote it.

**Variables:**

- `ASTDTC` would be the start as ISO 8601 text, read from an ISO 8601 date
  or from a two-digit day, three-letter month and four-digit year such as
  `04MAR2001`. A start written in any other spelling has no stated reading,
  so the run stops when it reaches one and no artifact is accepted.
- `ASTDT` would be the analysis start date, a month alone completed to its
  first day.

**Note:** a start that was never collected and a start in an unlisted
spelling are different defects: the first gives no analysis start, while
the second has no stated reading here.

## How to fix

Correct the collected start upstream when the intended date is clear:
`4 March 2001` recorded as `04MAR2001` reads as March 4, 2001. Declare a
spelling instead only when the sites use it and one of the closed date forms
reads it. The list names each spelling the sites use, with at most one of
the two slashed orders, so a study whose sites also write slashed dates
month first declares:

```yaml
derivation:
  date_text:
    source: CM.CMSTDAT
    forms: [iso, ddmonyyyy, mdy_slash]
```

No form reads `4 March 2001` itself. If text in an unlisted spelling is
intentionally treated as no analysis start, declare that outcome locally
with `invalid: null`.
