# Sum target lesion diameters at each tumor assessment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtr-sum-of-target-diameters.html)

**Goal:** build one record for each subject and scheduled tumor
assessment holding the visit order (`AVISITN`), the assessment
date (`ADT`), the summed diameters in millimeters (`AVAL`), the
counts behind the sum (`NMEAS` and `NTARGET`), and the
completeness flag (`ANL01FL`).

**Input:** scheduled tumor assessments with visit order and date,
tumor measurement records with lesion group, test code, numeric
result, and completion status, plus the lesion inventory selected
at study entry with lesion group and lesion identifier.

**Variables:**

- `AVISITN` is the numeric order of the assessment, carried from
  the scheduled assessment; it orders the assessments of a
  subject.
- `ADT` is the date of the assessment, carried from the scheduled
  assessment; missing when the assessment has no recorded date.
- `AVAL` is the sum of `TRSTRESN` over the target
  longest-diameter records at the assessment, those with `TRGRPID`
  of `TARGET` and `TRTESTCD` of `LDIAM` (longest diameter). A
  record with a missing result contributes nothing; `AVAL` is
  missing when the assessment measured no target lesion.
- `NMEAS` is how many target lesions the assessment measured: the
  count of non-missing `TRSTRESN` values among those records;
  missing when the assessment has no target longest-diameter
  records, which separates an assessment that was never performed
  from one that measured nothing.
- `NTARGET` is how many lesions were selected as target lesions at
  study entry: the count of inventory rows with `TUGRPID` of
  `TARGET`, counting `TULNKID`; it is the same at every assessment
  of the subject.
- `ANL01FL` is `Y` when every target lesion was measured, that is
  when `NMEAS` equals `NTARGET`; blank otherwise.

**Note:** lesions outside the target inventory never enter the
sum, and a sum over an incomplete set of target lesions is still
reported; `ANL01FL` marks the assessments where every target
lesion was measured.

**Standard:** ADaM | **Domain:** ADTR
