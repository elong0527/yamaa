# Demographics When No Subject Died

**Goal:** derive the date of death (`DTHDTC`) and the death flag (`DTHFL`)
for each subject, and submit the demographics as the Dataset-JSON file
`dm.json` beside its data-definition document.

**Input:** one row per subject carrying the reference start date and the date
of death collected on the death page.

**Variables:**

- `RFSTDTC` is the subject's reference start date; empty when the subject
  has none.
- `DTHDTC` is the collected date of death; empty when the subject did not
  die.
- `DTHFL` is `Y` when the subject has a date of death; empty otherwise.

**Note:** the dataset keeps both death variables even when no subject died,
because the standard expects them in every demographics dataset. The
data-definition document then marks each variable that no subject has a value
for as having no data; a variable empty for only some subjects, such as
`RFSTDTC`, is not marked.
