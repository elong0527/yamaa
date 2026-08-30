# ADaM ADLB: reject a viral load reported below the assay limit

This example uses collected viral-load results as they were reported to attempt
one record per subject and parameter:

- `AVAL` is the reported number of copies per millilitre.

One result was reported as being under a limit rather than as a number. The
limit it names is information, so reading it as that number, as zero, or as
absent would each replace a reported fact with a chosen one, and the
specification states none of them. The run must fail and no artifact is
accepted.
