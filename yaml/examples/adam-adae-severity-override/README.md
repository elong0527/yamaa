# ADaM ADAE: apply an approved severity correction

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-severity-override.html)

This example uses sample AE data and a `yamaa` specification to derive one row
per adverse event:

- `ASEV` is the collected severity in upper case, except for one event that an
  approved data correction reassigns to `SEVERE`;
- `ASEVN` is the numeric rank of `ASEV`, from `1` for mild to `4` for
  life-threatening.

`ASEVN` reads the severity after the correction, not the collected value, so a
corrected event carries both the corrected term and its matching rank.
