# ADaM ADSL: reject a treatment and dose taken from an unchosen record

This example uses collected demographics with exposure records to attempt one
record per subject:

- `TRT01A` and `TRT01DOSE` are the treatment the subject received and the dose
  they received of it, and are meant to describe one administration.

The subject has two administrations and nothing says which one the record
describes. Naming the pair together guarantees that both come from one
administration, and it cannot say which, so the run must fail and no artifact
is accepted.
