# ADaM ADTR: derive the current nadir

This example uses prepared target-lesion assessments to derive one row per
subject and assessment:

- `AVISITN` orders the assessments, and `ADT` is the assessment date when
  known;
- `AVAL` is the sum of measured target-lesion diameters, `NMEAS` is the number
  measured, and `NTARGET` is the number selected at study entry;
- `ANL01FL` is `Y` when every target lesion was measured and is empty
  otherwise;
- `NADIR` is the lowest `AVAL` from a complete assessment on or before the
  current assessment date. An assessment with no date has no cutoff and
  therefore no nadir.

An incomplete current assessment may retain a nadir established by an earlier
complete assessment.
