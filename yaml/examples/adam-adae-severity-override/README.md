# ADaM ADAE: apply an approved severity correction

This example uses sample AE data and a `yamaa` specification to derive one row
per adverse event:

- `ASEV` is the collected severity in upper case, except for one event that an
  approved data correction reassigns to `SEVERE`;
- `ASEVN` is the numeric rank of `ASEV`, from `1` for mild to `4` for
  life-threatening.

`ASEVN` reads the severity after the correction, not the collected value, so a
corrected event carries both the corrected term and its matching rank.
