# Name Each Laboratory Parameter and Visit From the Study's Terminology

**Goal:** derive the parameter name (`PARAM`) and the analysis visit
(`AVISIT`) of each laboratory result from their codes.

**Input:** laboratory results with the study and subject identifiers, the
test code, the numeric result and the visit number.

**Variables:**

- `PARAMCD` is the parameter code, the collected test code.
- `PARAM` is the name the study's parameter terminology gives the code,
  such as `Alanine Aminotransferase (U/L)` for `ALT`.
- `AVISITN` is the analysis visit number, the collected visit number.
- `AVISIT` is the name the study's visit terminology gives the visit
  number, such as `Week 2` for `2`; a visit number the terminology does
  not list is an unscheduled visit, `Unscheduled`.
- `AVAL` is the numeric result.

**Note:** each name is stated once, beside its code in the study's
terminology, and every dataset that names a parameter or a visit reads it
from there, so no dataset can spell a name differently.
