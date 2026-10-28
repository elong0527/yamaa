# Window Endpoints Decide Which Records Match

**Goal:** identify whether each integer value falls inside its own range.

**Input:** `spec_named.yaml`, `spec_source.yaml` and `spec_aggregate.yaml`
are independent alternatives over the same record and range files. Each
produces the same `ranges.csv` table with the same endpoint decisions.

**Variables:**

- `RANGE`, `SEQ` and `VALUE` retain the range identifier, sequence number
  and integer value from the input record.
- `BOTHN` is 1 when the value falls within both endpoints, including equality.
- `LEFTN` is 1 when the value includes the lower endpoint and excludes
  the upper.
- `RIGHTN` is 1 when the value excludes the lower endpoint and includes
  the upper.
- `NEITHERN` is 1 when the value is strictly inside both endpoints.
- `LOWEREX` is 1 when the value exceeds the lower endpoint, without an
  upper bound.
- `UPPEREX` is 1 when the value precedes the upper endpoint, without a
  lower bound.

**Note:** Every other answer is blank. A missing value or a missing stated
endpoint never matches. Equal endpoints match only with both included; reversed
endpoints never match. Omitting a bound leaves that side unrestricted.
