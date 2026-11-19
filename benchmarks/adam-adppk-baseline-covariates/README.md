# Baseline Covariates on Every Record of a Population PK Dataset

**Goal:** repeat on every dose record and every pharmacokinetic (PK)
concentration record of a population PK dataset the subject's baseline
covariates: age, sex and race with numeric codes (`SEXN`, `RACEN`),
weight and height with the body mass index and the body surface area
(`WTBL`, `HTBL`, `BMIBL`, `BSABL`), the baseline creatinine, alanine
aminotransferase and bilirubin (`CREATBL`, `ALTBL`, `TBILBL`), and the
creatinine clearance and estimated glomerular filtration rate (`CRCLBL`,
`EGFRBL`).

**Input:** exposure (EX) records, one per administration, with the
date/time (`EXSTDTC`), and PK concentration (PC) records with the
collection date/time (`PCDTC`); an administration gives a dose record and
a sample gives a concentration record. The subject-level dataset (ADSL)
has `AGE` in years, `SEX` and `RACE`. Vital signs (VS) hold the tests
`WEIGHT` (kg) and `HEIGHT` (cm), and laboratory (LB) records hold the
tests `CREAT` (umol/L), `ALT` (U/L) and `BILI` (umol/L) among others,
with the numeric result in `VSSTRESN` or `LBSTRESN`. A subject's baseline
record of a test is the one flagged `Y` in `VSBLFL` or `LBBLFL`.

**Variables:**

- `ASEQ`: the record's number within the subject, in the order of `ADTM`.
- `ADTM`: the date/time of the administration or of the sample.
- `EVID`: 1 for a dose record and 0 for a concentration record.
- `AGE`, `SEX`, `RACE`: as in ADSL.
- `SEXN`: 1 for `M` and 2 for `F`.
- `RACEN`: 1 for `AMERICAN INDIAN OR ALASKA NATIVE`, 2 for `ASIAN`, 3 for
  `BLACK OR AFRICAN AMERICAN`, 4 for `NATIVE HAWAIIAN OR OTHER PACIFIC
  ISLANDER`, 5 for `WHITE` and 6 for any other value.
- `WTBL`, `HTBL`: the baseline weight and height; empty when the subject
  has no baseline record of the test.
- `BMIBL`: `WTBL` divided by the square of `HTBL` in metres (`HTBL` / 100).
- `BSABL`: the Mosteller body surface area, the square root of
  `HTBL` x `WTBL` / 3600.
- `CREATBL`, `ALTBL`, `TBILBL`: the baseline results of `CREAT`, `ALT`
  and `BILI`; empty when the subject has no baseline record of the test.
- `CRCLBL`: the Cockcroft-Gault creatinine clearance in mL/min:
  (140 - `AGE`) x `WTBL` x 1.23 / `CREATBL` for a male subject, with 1.04
  in place of 1.23 for a female subject.
- `EGFRBL`: the CKD-EPI 2021 estimated glomerular filtration rate in
  mL/min/1.73 m2. With r = `CREATBL` / 88.42 / 0.9 for a male subject and
  `CREATBL` / 88.42 / 0.7 for a female subject, it is 142 x r^a x
  0.9938^`AGE`, times 1.012 for a female subject, where the exponent a is
  -1.2 when r is above 1 and otherwise -0.302 for a male and -0.241 for a
  female subject.

**Note:** no computed value is rounded, and a computed value is empty
when a value it is computed from is empty. Results that are not flagged
as baseline, and tests other than those named, are not used.
