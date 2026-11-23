# Derive Body Surface Area

**Goal:** carry each collected vital signs record through and add
one body surface area (BSA) record for each weight result, at the
weight's visit, from the subject's height.

**Input:** collected vital signs with height records coded
`HEIGHT` and weight records coded `WEIGHT`; height is collected
once per subject, in centimeters, and weight at each visit, in
kilograms.

**Variables:**

- `AVAL` keeps each collected result. On a body surface area
  record it is the Mosteller value: the square root of the
  subject's height in centimeters times the visit's weight in
  kilograms, divided by 3600. No record is added for a weight
  with no result, or when the subject has no height result.

**Note:** every weight of a subject uses the same height, whatever
visit collected it, so a visit without a height still gets a body
surface area. A subject with more than one height result stops the
run rather than choosing one. Body surface area is a parameter of
its own, so the data carries no `DTYPE`: that variable marks a
record added within a collected parameter.
