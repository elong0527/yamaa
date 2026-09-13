# Track the running nadir of the target-lesion sum

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtr-current-nadir.html)

**Goal:** carry each prepared target-lesion assessment forward and
add `NADIR`, the running lowest sum for the same subject. All
records share the fixed parameter `SDIAM` (`Sum of Target Lesion
Diameters (mm)`).

**Input:** one prepared assessment record per subject and visit,
carrying the visit number (`AVISITN`), assessment date (`ADT`),
measured sum (`AVAL`), measured-lesion count (`NMEAS`),
entry-selected count (`NTARGET`), and completeness flag
(`ANL01FL`).

**Variables:**

- `AVISITN`: numeric order of the assessment within the subject,
  carried from the input.
- `ADT`: date of the assessment, carried from the input; blank when
  the date is unknown.
- `AVAL`: sum of the measured target-lesion diameters in
  millimeter, carried from the input; blank when the assessment
  measured no target lesion.
- `NMEAS`: number of target lesions the assessment measured,
  carried from the input; blank when the assessment has no lesion
  record.
- `NTARGET`: number of target lesions selected at study entry,
  carried from the input.
- `ANL01FL`: completeness of the assessment, carried from the
  input; `Y` when every target lesion was measured, blank
  otherwise.
- `NADIR`: lowest `AVAL` among complete assessments (`ANL01FL` is
  `Y`) for the same subject dated on or before the current `ADT`,
  including the current assessment itself; the date cutoff is
  inclusive. Blank when the current `ADT` is blank, or when no
  complete assessment falls in the window.

**Note:** an incomplete current assessment keeps the nadir set by an
earlier complete one, while an assessment with a blank date has no
cutoff and therefore a blank nadir.

**Standard:** ADaM | **Domain:** ADTR
