# ADaM ADSL BMI function

This fixture is the suite's only coverage of the `function` expression: the
registered call lifecycle for a routine supplied by the project's global R
environment. `../adam-adsl-bmi-compute` derives the same value in portable YAML
and is the way arithmetic should be written; this fixture exercises the
extension point, not the formula.

Before evaluating the specification, the project loads `environment.R` into its
global R environment. Runtime selection and environment setup are project-level
configuration and therefore do not appear in `spec.yaml`.

The project function is:

```r
bmi <- function(weight_kg, height_cm, cm_per_m = 100) {
  weight_kg / (height_cm / cm_per_m)^2
}
```

The derivation passes `WEIGHTKG` and `HEIGHTCM` as direct variable arguments and
passes `cm_per_m` as the direct numeric literal `100`:

```yaml
function:
  name: bmi
  args:
    weight_kg: WEIGHTKG
    height_cm: HEIGHTCM
    cm_per_m: 100
```

A direct string argument is always a variable. A string literal uses an
explicit expression such as `{literal: kg}`.

The function returns one numeric BMI for each current row. R propagates the
missing height for `CATH-004`, producing a missing BMI without a function-level
handler. The expected output preserves all four source rows, which are shared
with `../adam-adsl-bmi-compute`. The project owns the `bmi` implementation, its
signature, and its environment dependencies.

The expected `BMI` of `25` for `CATH-002` is the rendered form of
`24.999999999999996`, which is what both R and the portable `compute` formula
return. `../adam-adsl-bmi-compute` explains where the last places go.
