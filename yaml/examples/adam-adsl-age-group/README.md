# ADaM ADSL: derive age group

Derives pooled age group flags for subject analysis records using DM:

- `AGE`: Subject age in years, directly copied from input.
- `AGEU`: Subject age units, directly copied from input.
- `AGEGR1`: Pooled age group character flag (`<18`, `18-64`, or `>64`),
  set to `Missing` when age is missing.
- `AGEGR1N`: Pooled age group numeric flag (`1`, `2`, or `3`), missing
  when age is missing.
