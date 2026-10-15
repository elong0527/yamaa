# Show Where Each Column Comes From

**Goal:** show where each column of the artifact comes from: the
artifact carries exactly the columns the specification declares, and
every declared column has a stated value, whether it is computed once
for all rows or separately by each row template.

**Input:** one `spec.yaml` over one lab-results file. Each record
carries a subject (`USUBJID`), a visit (`VISIT`), a test code (`ALT` or
`AST`), and a result that arrives as text. The file also carries a
sequencing field the specification never declares.

**Variables:**

- `PARAM`: the test's full name (`Alanine Aminotransferase` for `ALT`,
  `Aspartate Aminotransferase` for `AST`), or `Lowest Result of Visit`
  on the summary rows.
- `AVISIT`: the visit's display name (`WEEK 1 DAY 1`, `WEEK 2 DAY 8`);
  the summary rows replace it with `OVERALL`.
- `AVAL`: the result as a number.
- `DTYPE`: empty on the component rows; `LOWEST` on the summary rows.
- `FLAG`: `H` when the result is above 60, else `N`.

**Note:** the undeclared sequencing field never reaches the artifact,
and the working value behind `FLAG` is checked for completeness but
stays out of the artifact.
