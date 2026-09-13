# Reject a parameter that reads the dataset it builds

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-output-self-reference.html)

**Goal:** build analysis records for alanine aminotransferase
(`ALT`), aspartate aminotransferase (`AST`), and their ratio
(`ASTALT`), carrying `ADT` and `AVAL` for each subject, date, and
parameter.

**Input:** collected results carrying the collection date, test
code (either `ALT` or `AST`), and numeric result, plus a file of
analysis values.

**Variables:**

- `ADT` would be the collection date from the collected date.
- `AVAL` would be the collected result on each transaminase
  record, and the aspartate value read from the dataset being
  built divided by the alanine result on the ratio record, missing
  when either result is absent.

The ratio read matches subject, date, and the code `AST` in the
dataset being built; the denominator is the alanine result, with
division by zero returning missing. The values the ratio reaches
for are the values this run is producing under the same name.
Nothing can distinguish the record being written from the record
being read back, so the run is rejected before any data is read
and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

First decide whether the aspartate values the ratio reads are already final.
If they are, they belong to a finished dataset that this run merely reads, so
give that source a name of its own and keep the name of the dataset being
built for the dataset being built:

```yaml
datasets:
  LB: input/lb.csv
  ADLBIN: {path: input/adlb.csv, types: {AVAL: float, ADT: date}}
```

Renaming the declaration alone leaves the read pointing at a name that no
longer exists, so change where it reads from as well:

```yaml
mapping_from:
  dataset: ADLBIN
```

The read then uses a completed dataset by its own name, which is an ordinary
source like any other. If the aspartate values are not final, splitting the
work applies instead: complete and write the transaminase parameters before
the run that reads them starts.
