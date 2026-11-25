# Read a Column-Aligned Transfer File

**Goal:** list each subject's height (`HEIGHT`), weight (`WEIGHT`) and visit
date (`VISITDT`) from a legacy vital-signs transfer.

**Input:** one `spec.yaml` reading `input/vitals.csv`, a plain-text transfer
whose fields sit at fixed character positions: the subject identifier in
positions 1 to 4, the height in 5 to 7, the weight in 8 to 14 and the visit
date in 15 to 24, with no separator between them and no quoting. Its first
line is a column ruler, not a subject.

**Variables:**

- `SUBJID` is the subject identifier as collected, leading zeros kept.
- `HEIGHT` is the height in centimetres, a whole number.
- `WEIGHT` is the weight in kilograms; it is missing when its positions hold
  only spaces.
- `VISITDT` is the date of the visit.

**Note:** the column ruler is dropped before the first subject is read. Each
value is read from its own positions with the spaces padding it removed, so
the weight right-aligned in seven positions reads as the number it holds.
