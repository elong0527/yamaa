# Reject Two Text Operations Written as One Value

**Goal:** standardize the collected race (`RACE`) to upper case, with each
run of spaces reduced to one.

**Input:** collected demographics, one record per subject, with the race as
entered.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `RACE` would be the collected race in upper case, with single spaces.

**Note:** the race is given two operations at once, a change to upper case
and a reduction of repeated spaces, each reading the collected text. Nothing
says which applies first or whether the second reads the first one's
result, so the run is rejected before any data is read and no artifact is
accepted.

## How to fix

Write one operation per value, and chain them through a column of their own:

```yaml
- name: RACESP
  type: str
  label: Race, Single Spaces
  derivation: {str_replace: {source: RAW.RACE, pattern: ' +',
                             replacement: ' '}}
- name: RACE
  type: str
  label: Race
  derivation: {str_case: {source: RACESP, to: upper}}
```

Leave `RACESP` out of the published columns.
