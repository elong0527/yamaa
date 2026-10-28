# Turn protocol metadata into trial summary parameters

**Goal:** create a trial summary (TS) record for each protocol parameter,
with its value (`TSVAL`), controlled term code (`TSVALCD`) and reference
terminology (`TSVCDREF`).

**Input:** a protocol-authoring system export, with a study identifier,
trial title, phase, randomization decision and planned enrollment.

**Variables:**

- `TSSEQ` orders the parameters: title, phase, randomization, then planned
  enrollment.
- `TSPARMCD` and `TSPARM` identify and name each parameter.
- `TSVAL` holds the protocol title, the standard phase term, `Y` for a
  randomized trial or `N` otherwise, or the planned enrollment as text.
- `TSVALCD` holds the NCI concept code for phase and randomization; it is
  empty for the title and planned enrollment.
- `TSVCDREF` is `CDISC` for those controlled terms and empty for uncoded
  values.

**Note:** each value fits in one trial summary record. Splitting long
values and describing value-level submission metadata are separate tasks.
