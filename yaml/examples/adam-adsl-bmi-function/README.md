# ADaM ADSL: compute BMI by calling a routine the project supplies

This example uses sample DM data and a `yamaa` specification to derive one row
per subject:

- `HEIGHTCM` and `WEIGHTKG` are the collected height and weight;
- `BMI` is body mass index, calculated by a routine the project provides rather
  than by a formula written in the specification. The specification names the
  routine and its exact contract and says which values to pass it. The project
  environment selects one R runtime and binds that routine to `bmi`.
