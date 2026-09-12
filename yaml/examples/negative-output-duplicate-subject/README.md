# ADaM ADSL: reject a repeated demographics record

This example uses collected demographics to attempt one record per subject:

- `AGE` is the age collected at screening;
- `SEX` is the collected sex.

One subject was entered twice with different ages. Each subject may appear once
in the result, and keeping either record or merging the two would report an age
the collected data does not support. The expected file records the completed
rows presented to that check, but the repetition still rejects the run and no
artifact is accepted.

## How to fix

Reconcile the two source records for `P7-722` and correct the governed
demographics input so it contains one supported age for the subject. If
multiple source records are legitimate, use a unique subject inventory as the
row driver and declare an ordered record-selection rule for the demographics
record; do not rely on source order to discard one. The completed output must
contain exactly one row for each `[STUDYID, USUBJID]` key.
