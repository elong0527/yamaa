# Tell the study eye from the fellow eye

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adoe-study-eye.html)

**Goal:** one ophthalmic row per collected ophthalmic examination
(OE) measurement, adding `AFEYE`.

**Input:** ophthalmic measurements carrying sequence number
(`OESEQ`), test code (`PARAMCD`), laterality (`OELAT`), and
numeric result (`AVAL`), together with the subject's assigned
eye from the subject-level analysis dataset (ADSL). A
measurement collected without a value is kept with its value
empty.

**Variables:**

- `AFEYE` is the eye's role in the study: `Study Eye` when the
  measured eye matches the subject's assigned eye, `Both Eyes`
  for a bilateral measurement when the subject has an assigned
  eye, and `Fellow Eye` for the opposite eye. Either eye counts
  as the study eye when both eyes are assigned and the collected
  laterality is present. When the assigned eye or the collected
  laterality is missing, `AFEYE` stays empty.

**Note:** the assigned eye belongs to the subject, so the same eye
is the study eye at every visit; only the collected laterality
moves a record between roles.

**Standard:** ADaM | **Domain:** ADOE
