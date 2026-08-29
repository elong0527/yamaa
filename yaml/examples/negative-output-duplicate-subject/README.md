# ADaM ADSL: reject a repeated demographics record

This example uses collected demographics to attempt one record per subject:

- `AGE` is the age collected at screening;
- `SEX` is the collected sex.

One subject was entered twice with different ages. Each subject may appear once
in the result, and keeping either record or merging the two would report an age
the collected data does not support. The expected file records the completed
rows presented to that check, but the repetition still rejects the run and no
artifact is accepted.
