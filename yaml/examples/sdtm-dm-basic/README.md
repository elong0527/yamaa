# SDTM DM: build one subject record from collected data

This example uses collected long-form clinical data and a `yamaa` specification
to derive one record per subject. The `rows` entry uses each subject's SEX item
record only to establish the output grain; all values are derived with their
column declarations:

- `USUBJID` and `SUBJID` identify the subject;
- `SEX` is the collected sex translated into the standard `M`, `F`, and `U`.
  A sex that was not collected, and one reported as something the study does
  not recognise, both become `U`;
- `AGE` is the collected age as a whole number, and is empty for a subject
  whose age was never collected;
- `ARM` is the planned treatment arm, and a subject with no arm collected is
  `Unassigned`;
- `ACTARM` is the arm the subject actually received, which this study takes to
  be the planned one, so it repeats the resolved `ARM` including its
  `Unassigned` fallback.

Every subject represented by a SEX item record yields a record even when its
value or other items were not collected, so a sparsely collected subject
appears with the substituted values rather than being dropped.
