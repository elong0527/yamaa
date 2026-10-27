# Demographics Submitted in SAS Transport with SDTMIG 3.3

**Goal:** build one DM record per subject carrying `RFSTDTC`, `SITEID`,
`AGE`, `AGEU`, `SEX`, `RACE` and `COUNTRY`, and submit it as the SAS
transport file `dm.xpt` beside its data-definition document.

**Input:** one row per subject carrying the site, reference start date, age,
sex, race and country.

**Variables:**

- `RFSTDTC` is the subject's reference start date; empty when the subject
  has none.
- `SITEID` is the collected site identifier.
- `AGE` is the collected age in whole years.
- `AGEU` is fixed to `YEARS`.
- `SEX`, `RACE` and `COUNTRY` are as collected.

**Note:** in the transport file each text variable is exactly as wide as its
longest value, and the data-definition document reports that width as the
variable's length instead of the largest length the study allows. A date
travels as `YYYY-MM-DD` text, and an empty value is blanks.

The `project.yaml` guide declaration is SDTMIG 3.3; the data-definition
document records it and points to the SAS Transport version 5 dataset.
