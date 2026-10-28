# Tabulate nonclinical body weights with SENDIG 3.1.1

**Goal:** build one body-weight finding per weighing of an animal, carrying
`BWSEQ`, `BWTESTCD`, `BWTEST`, the result as collected and standardized, and
`BWDTC`, and submit it as the SAS transport file `bw.xpt` beside its
data-definition document.

**Input:** the weighings the test facility's in-life data-collection system
records, one row per animal per weighing, with the weighing occasion (routine
or terminal), date, weight and unit.

**Variables:**

- `BWSEQ` numbers each animal's weighings from 1 in date order, whatever
  order the facility exported them in.
- `BWTESTCD` is `BW` for a routine weighing and `TERMBW` for the terminal
  weighing taken before necropsy; `BWTEST` is `Body Weight` or
  `Terminal Body Weight`.
- `BWORRES` and `BWORRESU` are the weight and unit as collected.
- `BWSTRESN` is the collected weight as a number, and `BWSTRESC` that number
  as text, so a collected `281.0` is standardized to `281`.
- `BWSTRESU` is the collected unit; no weight is converted.
- `BWDTC` is the date the animal was weighed.

**Note:** the study declares the nonclinical implementation guide SENDIG
3.1.1, and the data-definition document records it. The test facility
collects every result, so the document names no collecting party or
annotated case report form for it; a nonclinical study identifies its test
facility in its trial summary instead.
