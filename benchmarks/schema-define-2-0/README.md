# Submit demographics with the older metadata format

**Goal:** carry the demographics identifiers and collected values with
Define-XML 2.0.0 metadata and a SAS transport file.

**Input:** demographic capture records exported from an electronic case
report form, with subject identifiers, reference start dates, sites, ages,
sex, race and country.

**Variables:**

- `SUBJID` is the subject identifier within the study.
- `RFSTDTC` is the subject's reference start date, blank when unavailable.
- `SITEID` identifies the site.
- `AGE` is the collected age, with `AGEU` fixed to `YEARS`.
- `SEX`, `RACE` and `COUNTRY` retain the recorded values.

**Note:** the submission uses the older metadata representation: its guide
is recorded on the metadata version, its dataset class is an attribute,
and collected origins are marked CRF. The dataset values and transport
layout are the same as in the corresponding newer-format submission.
