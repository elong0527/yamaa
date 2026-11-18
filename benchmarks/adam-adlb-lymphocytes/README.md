# Derive absolute differential records

**Goal:** add absolute differential records (`LYM`, `NEUT`,
`MONO`, `EOS`, `BASO`) whose analysis value (`AVAL`) is the
white blood cell count times the matching differential fraction.

**Input:** SDTM laboratory records (`LB`) carrying the subject
identifier (`USUBJID`), visit (`VISIT`), test code (`LBTESTCD`),
test name (`LBTEST`), and numeric result (`LBSTRESN`): white blood
cell counts (`WBC`), differential fractions (`LYMLE`, `NEUTLE`,
`MONOLE`, `EOSLE`, `BASOLE`), a few lab-reported absolute
differentials, and unrelated parameters.

**Variables:**

- `PARAMCD` and `PARAM`: the collected test code and name on a
  collected record; on an added absolute record, the absolute
  test code (`LYM`, `NEUT`, `MONO`, `EOS` or `BASO`) and its
  name with its unit, such as `Lymphocytes Abs (10^9/L)`.
- `AVAL`: the collected result on a collected record; on an
  added absolute record, the `WBC` count times the matching
  differential fraction for the same subject and visit.

**Note:** one record is added per subject, visit, and
differential: only when both a `WBC` count and that differential's
fraction are present for the subject and visit, and no record of
the new absolute parameter is already there. A differential with
a fraction but no `WBC` count gains nothing, and an existing
absolute record blocks only its own differential. Each
contributing parameter may appear at most once within a subject
and visit; a subject and visit with repeated `WBC` or fraction
results stops the run instead of using a value picked by amount
or position.
