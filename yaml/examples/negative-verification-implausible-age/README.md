# ADaM ADSL: reject an implausible age

This example uses collected demographics to attempt one record per subject:

- `AGE` is the age collected at screening, which the study restricts to adults
  under one hundred and one.

One collected age falls outside that range. Carrying it forward would publish a
value the study's own entry criteria exclude, so the run must fail and no
artifact is accepted.

## How to fix

Query and correct the source age for `P7-732` if `214` is a data-entry error,
then rerun the unchanged range verification. If the protocol genuinely permits
the confirmed value, revise the verification boundary to the protocol's
documented limit. Do not remove or widen the check merely to make an
unconfirmed value pass.
