# Resolve items within their collection form

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/odm-form-scoped-item-resolution.html)

**Goal:** derive `LBTESTCD`, `LBTEST`, `LBCAT`, `LBORRES`, and
`LBDTC` for one record per reported laboratory result, reading each
item only within the form that collected it.

**Input:** long-form Operational Data Model (ODM) data with one row
per item, carrying the collection form (`FormOID`), the item
(`ItemOID`), and the stored value (`Value`). Result rows use item
`IT.LB.RESULT`; date rows use item `IT.LB.LBDTC`, the same
identifier on every form.

**Variables:**

- `LBTESTCD` is the short test code for the collection form.
- `LBTEST` is the test name for the collection form.
- `LBCAT` is the test category for the collection form.
- `LBORRES` is the reported result, the value on the result row of
  that form.
- `LBDTC` is the collection date from the same form, the value on
  that form's date row. A form with no date row keeps its result
  with a missing date.

**Note:** form identity is part of the collection setting: a date
row from one form never supplies the date for a result on another
form at the same subject and visit. Records follow the declared
form order within each study and subject.

**Standard:** SDTM | **Domain:** LB
