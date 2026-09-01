# ADaM ADTR: sum the target lesion diameters at each assessment

This example uses a schedule of tumour assessments, the lesions measured at
each of them, and the lesion inventory selected at study entry to derive one
record per subject and assessment:

- `ADT` is the date of the assessment when known, and `AVISITN` orders the
  assessments within a subject;
- `AVAL` is the sum of the target lesion diameters measured at it. A lesion
  the assessment did not measure contributes nothing, so the sum is over
  whatever was measured; it is empty only when the assessment produced no
  measurement of any target lesion;
- `NMEAS` is how many target lesions the assessment measured and `NTARGET` how
  many were selected as target lesions at entry. `NMEAS` is empty rather than
  zero when the assessment left no lesion record at all, which separates an
  assessment that was never performed from one that measured nothing;
- `ANL01FL` is `Y` when every target lesion was measured, and is empty
  otherwise.

Lesions outside the target inventory are measured and recorded too, and are
never part of this sum.

A sum over an incomplete set of lesions is still reported, because whether it
may be compared with an earlier one is a question for the analysis rather than
for the record. `ANL01FL` is what carries the answer.
