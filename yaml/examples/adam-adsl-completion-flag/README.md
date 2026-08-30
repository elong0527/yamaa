# ADaM ADSL: flag the subjects who completed the study

This example uses a subject-level dataset and the disposition dataset with a
`yamaa` specification to derive one row per subject:

- `TRTSDT` is the date of the subject's first dose, carried across. A subject
  never dosed has none;
- `COMPFL` is `Y` when the subject has a disposition record whose standardized
  outcome is a completion, and `N` otherwise. A subject whose only disposition
  record reports a discontinuation reason, and a subject with no disposition
  record at all, are both `N`.

A completion later withdrawn by a newer record does not qualify a subject: the
disposition record must still stand. The flag answers whether the record
exists, not how the study ended, so a subject who both completed and later
discontinued from a follow-up period keeps `Y` for the completion.
