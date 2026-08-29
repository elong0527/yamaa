# ADaM ADSL: reject a malformed subject reference

This example uses one demographics record to attempt one subject record:

- `SUBJREF` is meant to combine the collected site and subject identifiers,
  separated by a colon.

The replacement name contains several inputs and punctuation instead of one
input name. Treating that text as executable would make its meaning ambiguous,
so the run must fail and no artifact is accepted.
