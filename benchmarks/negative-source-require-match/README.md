# Reject a Subject Without a Dosed Exposure Record

**Goal:** derive each subject's date of first exposure to treatment
(`TRTSDT`), where every subject must have received a dose.

**Input:** demographics, and exposure records carrying each administration's
start date and dose.

**Variables:**

- `TRTSDT` would be the start date of the subject's earliest administration
  with a dose above zero. Every subject is required to have one, so a subject
  whose administrations all have a zero dose, or who has none, rejects the run
  with no artifact accepted, instead of leaving the date blank.

## How to fix

Correct the exposure data when subject `YAMAA-01-103` was dosed: its only
administration records a zero dose. If a subject without a dosed
administration is acceptable, drop `require_match: true`:

```yaml
derivation:
  source:
    variable: EX.EXSTDTC
    filter: "EX.EXDOSE > 0"
    order_by: [EX.EXSTDTC]
    keep: first
```

The date is then missing for such a subject, and the run counts each one under
`absent`. Declaring `absent` beside `require_match: true` is refused as a
contradiction.
