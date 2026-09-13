# Consolidate four collection forms into one dataset

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-lb-multiform.html)

**Goal:** one record per reported result from serum, skin-biopsy,
saliva, and tape-strip forms, carrying `LBTESTCD`, `LBTEST`,
`LBCAT`, `LBSPEC`, `LBLOC`, `LBORRES`, `LBORRESU`, `LBSTRESC`,
`LBSTRESN`, `LBSTRESU`, `LBSTAT`, `VISITNUM`, and `LBDTC`.

**Input:** Operational Data Model (ODM) extract in long form, one
record per collected item, with the collected entry in the value
field; each form also carries its collection date as a sibling
item record.

**Variables:**

- `VISITNUM` is the visit number, one of `1`, `2`, `3`, or `99`,
  with the visit name from the same item.
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
- `LBORRES` is the collected result, exactly as reported,
  including text such as `NOT DONE`.
- `LBORRESU` is the collected unit: `ng/mL`, or `CYCLE` for biopsy
  results.
- `LBSTRESC` repeats the reported result in standard form,
  including `NOT DONE`.
- `LBSTRESN` is the numeric form of the reported result; missing
  when the result is `NOT DONE`.
- `LBSTRESU` is the standard unit, matching the collected unit.
- `LBSTAT` is `NOT DONE` when the reported result is `NOT DONE`;
  blank otherwise.
- `LBDTC` is the collection date from the date item on the same
  form.

**Note:** an item with no reported value produces no record, while
a collected zero is kept as a real result; each record keeps the
date of the form it came from, so a form collected twice at one
visit keeps each occurrence's own date.

**Standard:** SDTM | **Domain:** LB
