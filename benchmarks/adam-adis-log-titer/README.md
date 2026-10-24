# Derive log10 neutralizing antibody titers

**Goal:** carry each quantified neutralizing antibody titer into `AVAL` on
the base-10 logarithmic scale, the scale titers are analyzed on, with one
record for each subject, parameter and visit.

**Input:** immunogenicity results with one neutralizing antibody titer per
subject and visit, alongside the assay's lower limit of quantitation.

**Variables:**

- `PARAMCD`: `NABLOG10`, the base-10 logarithm of the neutralizing antibody
  titer.
- `AVAL`: the base-10 logarithm of the titer, at full precision, and blank
  when the titer is blank.

**Note:** a titer below the assay's lower limit of quantitation, a titer of
zero among them, has no record: zero has no logarithm, and a titer under
the limit is not a measured value to transform.
