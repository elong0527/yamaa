# ADaM ADVS: derive body mass index

The artifact contains collected ADVS records and one derived body mass index
parameter per eligible subject and visit:

- `PARAMCD` retains each collected code and is `BMI` for the derived record;
- `PARAM` retains each collected name and is body mass index for the derived
  record;
- `AVAL` retains each collected value. For BMI, it is weight in kilograms
  divided by the square of the subject's once-measured height in metres. No BMI
  record is added when weight is missing or when height is missing or zero;
- `DTYPE` is `CALCULATION` on a derived BMI record and missing on a collected
  record.

Input containing a pre-existing BMI parameter is rejected. The derived BMI
row is generated for each collected weight record using one height per subject.
The formula and once-measured-height behavior follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`e32e5689d7fd03e224ddbcfc369c332c5df837d9`, `R/derive_param_bmi.R`.
