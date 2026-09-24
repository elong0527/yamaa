# Tumor Measurements

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-tr-tumor-measurements.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per collected tumor assessment, carrying
the collected test, result, unit, method, evaluator, and visit
through and adding the standardized numeric result, text result,
and standard unit: `TRTESTCD`, `TRTEST`, `TRORRES`, `TRORRESU`,
`TRSTRESN`, `TRSTRESC`, `TRSTRESU`, and `TRSTAT`.

**Input:** collected lesion assessments with one row per lesion per
visit, carrying the collected result and unit plus study, subject,
sequence (`TRSEQ`), link, method, evaluator, visit, and date
identifiers.

**Variables:**

- `TRLNKID` is the link identifier tying the assessment to its
  lesion in the tumor identification (TU) domain; target,
  non-target, and new lesions each keep their own identifier.
- `TRTESTCD` is the test code as collected: `DIAMETER` for a measured
  lesion or `TUMSTATE` for a lesion state.
- `TRTEST` is the test name as collected: `Diameter` or `Tumor State`.
- `TRORRES` is the result exactly as collected, never overwritten:
  a diameter, `TOO SMALL TO MEASURE`, a state such as `PRESENT`,
  or empty when the lesion was not assessed.
- `TRORRESU` is the unit exactly as collected: `mm` or `cm` for a
  diameter, empty otherwise.
- `TRSTRESC` is the standardized character result: a diameter in
  mm written as text, `5` for a lesion too small to measure, a
  lesion state kept as collected, and empty when the lesion was
  not assessed.
- `TRSTRESN` is the standardized numeric result in mm: a diameter
  passes through, a value collected in `cm` is multiplied by 10,
  and a lesion too small to measure takes the study convention of
  5 mm; it stays empty for lesion states and for lesions not
  assessed.
- `TRSTRESU` is `mm` whenever a standardized numeric result
  exists, empty otherwise.
- `TRSTAT` is `NOT DONE` for a lesion not assessed at a visit,
  empty otherwise.
- `TRMETHOD` is the measurement method as collected.
- `TREVAL` names the evaluator as collected.
- `VISITNUM` is the visit number as collected.
- `TRDTC` is the assessment date as collected, empty when the
  lesion was not assessed.

**Note:** a lymph-node target lesion contributes its short axis as
its diameter, following the study's response criteria; that axis is
already the value the sum of diameters needs, so it is carried like
any other diameter. A lesion too small to measure is not a zero: it
takes the study's conventional 5 mm, and a lesion not assessed
stays out of every standardized column rather than counting as
zero.

**Standard:** SDTM | **Domain:** TR
