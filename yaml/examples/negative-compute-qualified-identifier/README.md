# ADaM ADEX: reject a doubled dose read straight from exposure

This example uses a subject-treatment inventory with its component exposure
records to attempt one record per subject:

- `DOSEDBL` is meant to be twice the administered dose.

A subject has several exposure records, so a formula naming the exposure dose
does not say which record it means. Silently choosing one, or totalling them,
would each give a different result from the same specification, so the run must
fail and no artifact is accepted. A formula computes from values the record
already carries, and a value taken from another source becomes one of those
first.
