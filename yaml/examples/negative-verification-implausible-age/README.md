# ADaM ADSL: reject an implausible age

This example uses collected demographics to attempt one record per subject:

- `AGE` is the age collected at screening, which the study restricts to adults
  under one hundred and one.

One collected age falls outside that range. Carrying it forward would publish a
value the study's own entry criteria exclude, so the run must fail and no
artifact is accepted.
