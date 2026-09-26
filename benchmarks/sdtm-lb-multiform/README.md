# Multiform Labs

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-multiform.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one record per reported result from serum, skin-biopsy,
saliva, and tape-strip forms, carrying `LBTESTCD`, `LBTEST`,
`LBCAT`, `LBSPEC`, `LBLOC`, `LBORRES`, `LBORRESU`, `LBSTRESC`,
`LBSTRESN`, `LBSTRESU`, `LBSTAT`, `VISITNUM`, and `LBDTC`.

**Input:** Operational Data Model (ODM) extract in long form, one
record per collected item, with the collected entry in the value
field; each form also carries its collection date as a sibling
item record.

**Variables:**

- `VISITNUM` is the visit number: `1` screening, `2` baseline, `3`
  day 21; each unscheduled visit is numbered after the baseline visit
  per occurrence (`2.01`, `2.02`), so repeated unscheduled visits stay
  distinct. `VISIT` is the matching visit name.
- `LBTESTCD` is the short test code for the collected item:
  `VITD25OH`, `IL13`, or `CAMPPRO`.
- `LBTEST` is the full test name for that code: `25-Hydroxyvitamin
  D`, `Interleukin 13 mRNA`, or `Cathelicidin Protein`.
- `LBCAT` is the test category for that code: `CHEMISTRY`,
  `GENE EXPRESSION`, or `ANTIMICROBIAL PEPTIDE`.
- `LBSPEC` is the specimen type of the form the result came from:
  `SERUM`, `SKIN BIOPSY`, `SALIVA`, or `TAPE STRIP`.
- `LBLOC` is the sampled site where the form distinguishes one:
  `LESIONAL` or `NON-LESIONAL` for biopsy and tape-strip results;
  blank for serum and saliva results.
- `LBORRES` is the collected result, exactly as reported; blank
  when the test was not done.
- `LBORRESU` is the unit fixed for each form: `CYCLE` for biopsy
  results and `ng/mL` otherwise; blank when the test was not done.
- `LBSTRESC` repeats the reported result in standard form; blank
  when the test was not done.
- `LBSTRESN` is the numeric form of the reported result; missing
  when the test was not done.
- `LBSTRESU` is the standard unit, the same as `LBORRESU`; blank
  when the test was not done.
- `LBSTAT` is `NOT DONE` when the test was not done; blank
  otherwise.
- `LBDTC` is the collection date from the date item on the same
  form.

**Note:** an item with no reported value produces no record, while
a collected zero is kept as a real result; an entry of `NOT DONE`
still produces a record, with the result and unit fields left blank
and the status recorded. Each record keeps the date of the form it
came from, so a form collected twice at one visit keeps each
occurrence's own date.

**Standard:** SDTM | **Domain:** LB
