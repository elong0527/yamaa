# Reject a Boolean Dataset-JSON Column

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file that also
holds a safety population flag, `SAFFL`, as a column of true and false
values.

**Variables:**

- `SITEID` would identify the site where the subject enrolled. The rejected
  file produces no output value.
- `AGE` would give the subject's age in whole years. The rejected file
  produces no output value.

A study's columns hold text, whole numbers, decimals, dates and date-times;
none holds a true or false value, so no type can be chosen for the flag
without converting it. Every column of the file is read, so the file is
rejected while it is read, naming the column, even though the listing does
not use the flag, and no artifact is accepted.

## How to fix

Ask the sender to write the flag as the controlled terminology the standard
uses for it, the text `Y` or `N`, in a string column:

```json
{"itemOID": "IT.DM.SAFFL", "name": "SAFFL",
 "label": "Safety Population Flag", "dataType": "string"}
```
