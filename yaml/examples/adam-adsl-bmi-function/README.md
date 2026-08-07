# ADaM ADSL BMI function

This fixture derives body mass index through the `function` expression. Before
evaluating the specification, the project loads `environment.R` into its global
R environment. Runtime selection and environment setup are project-level
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
handler. The expected output preserves all four source rows.

This fixture establishes only the registered call lifecycle. The project owns
the `bmi` implementation, its signature, and its environment dependencies.
