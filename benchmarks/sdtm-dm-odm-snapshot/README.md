# Demographics from an EDC Snapshot Export

**Goal:** build one Demographics (DM) record per subject with the birth date
(`BRTHDTC`), age (`AGE`, `AGEU`) and sex (`SEX`) collected on the
demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, saved as a snapshot: the current
value of every collected item, with the audit trail of each change, the
investigator's electronic signature and the site's comments kept beside the
data.

**Variables:**

- `SUBJID` is the subject's key in the export, and `USUBJID` joins the study
  identifier and that key with a hyphen.
- `BRTHDTC` is the birth date as far as it was collected: a year and month
  when the day was unknown, and missing when the form records the date as
  not collected.
- `AGE` is the collected age in whole years, and `AGEU` is `YEARS` whenever
  `AGE` is present.
- `SEX` is the collected sex code, `F` or `M`, as the form holds it now: a
  value corrected after it was first entered gives the corrected value.

**Note:** each variable reads the subject's current answer on the form. The
audit trail, the signature and the comments describe the data and add no
answers, so a corrected answer is read once, as it now stands.
