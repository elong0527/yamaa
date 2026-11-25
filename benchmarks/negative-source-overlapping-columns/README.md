# Reject Column Positions That Overlap

**Goal:** list each subject's height (`HEIGHT`), weight (`WEIGHT`) and visit
date (`VISITDT`) from a legacy vital-signs transfer whose fields sit at fixed
character positions.

**Input:** a plain-text transfer with the subject identifier in positions 1
to 4, the height in 5 to 7, the weight in 8 to 14 and the visit date in 15
to 24, after a column ruler line. The layout the input states gives the
weight positions 7 to 14, so its first position is also the height's last.

**Variables:**

- `HEIGHT` would be the height in centimetres.
- `WEIGHT` would be the weight in kilograms.
- `VISITDT` would be the date of the visit.

One character cannot belong to two values, and reading it into both would
put the height's last digit in front of every weight, so the layout is
rejected before the transfer is read and no artifact is accepted.

## How to fix

Take each field's first and last positions from the transfer's own layout
document, and check that each field begins after the previous one ends:

```yaml
- name: HEIGHT
  columns: 5-7
- name: WEIGHT
  columns: 8-14
```
