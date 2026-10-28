# Adjacent Windows Share an Endpoint

**Goal:** assign each integer value to one of two adjacent windows.

**Input:** eight records, with values at, inside and outside the windows
1 to 8 and 8 to 15, plus a missing value. Three alternative specifications
produce the same table.

**Variables:**

- `RANGE`, `SEQ` and `VALUE` retain the range identifier, sequence number
  and integer value from the input record.
- `LEFTN` is the window number when the lower endpoint is included and the
  upper excluded: value 8 belongs to window 2.
- `RIGHTN` is the window number when the lower endpoint is excluded and the
  upper included: value 8 belongs to window 1.

A record outside both windows, or missing its value, has a blank answer.
Each record at the shared endpoint belongs to exactly one window.
