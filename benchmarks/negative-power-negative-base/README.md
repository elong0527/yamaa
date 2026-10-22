# Reject a Body Surface Area From a Negative Weight

**Goal:** derive each subject's body surface area (`BSA`) from the baseline
weight (`WEIGHTKG`) and height (`HEIGHTCM`) by the DuBois formula.

**Input:** baseline vital signs with one record per subject holding the
weight in kilograms and the height in centimetres.

**Variables:**

- `WEIGHTKG` and `HEIGHTCM` are the baseline weight and height as collected.
- `BSA` would be the body surface area in square metres,
  0.007184 x weight^0.425 x height^0.725.

One subject's weight was entered as -72.5 kg. A negative number raised to a
fractional power has no real value, and giving that subject no body surface
area would hide an entry error behind an empty value. The run is rejected,
and no artifact is accepted.

## How to fix

Correct the weight at the source: a weight is never negative, so the minus
sign is an entry error. Query the site, then export the corrected record:

    STUDYID,USUBJID,WEIGHTKG,HEIGHTCM
    CTX,CTX-61,68.2,171.5
    CTX,CTX-62,72.5,180.0
    CTX,CTX-63,59.0,162.3
