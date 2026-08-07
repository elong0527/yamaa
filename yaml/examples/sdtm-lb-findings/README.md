# SDTM LB Findings row construction

The `odm.csv` fields follow the ODM 2.0 clinical-data hierarchy. The row
templates filter the long data directly using SQL predicates. Each calcium or
creatinine `ItemData` value emits one flat LB record.

The Day 8 source contains a calcium `ItemData` with a missing value, so the
calcium template emits no Day 8 record. The creatinine template also encounters
`NOT DONE` on Day 15. It retains that collected text in `LBORRES`, while the
numeric `LBSTRESN` derivation handles conversion failure with a missing value.
`ODM.IT.LB.LBDTC` is resolved using the current result row's ODM context keys.
This reduction occurs during row construction and is therefore allowed.

Portable expression semantics used by this example:

- `row_number` partitions by `group_by` and sorts ascending by
  `order_by`. Ties must preserve row-template order and then base-record order.

After the four rows are constructed, shared column derivations populate
`STUDYID`, `DOMAIN`, `USUBJID`, and `LBDTC`. `LBSEQ` is assigned last so that the
final key is unique.
