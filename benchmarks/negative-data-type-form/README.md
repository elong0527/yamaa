# Reject a Partial Birth Date Submitted as a Complete Date

**Goal:** carry each subject's collected date of birth (`BRTHDTC`) into
the demographics domain, submitted as a complete ISO 8601 date.

**Input:** collected demographics with the study and subject identifiers
and the date of birth as ISO 8601 text, empty when it was not recorded.

**Variables:**

- `BRTHDTC` would be the collected date of birth, copied as text. It is
  described to reviewers as a complete date, a year, month and day, so a
  birth date recorded to the month alone contradicts that description;
  once the birth dates are complete the run is rejected and no artifact
  is accepted. A birth date that was not recorded is left empty.

## How to fix

Lead with the collection: when the form records birth dates only to the
month or the year for some subjects, the variable holds partial dates,
and its submission type says so:

```yaml
submission:
  core: Perm
  data_type: partialDate
  origin:
    type: Collected
```

When every birth date was in fact collected in full, correct the record
that lost its day at its source instead. Do not drop the declared type to
`text`: reviewers rely on it to read the values as ISO 8601 dates.
