# SDTM RELREC related records

This example relates one adverse event to two concomitant medications. AE and
CM use the same sponsor-defined link identifier, which becomes `RELID`.

RELREC has no single base dataset. Each row definition declares its row-driving
`dataset`; the constructed rows are appended in specification order. The AE
definition emits one RELREC row from the linked AE record, and the CM definition
emits two rows from the linked CM records. The declared `str` type converts the
numeric sequence values to the character `IDVARVAL` required by RELREC. Source
record order is preserved within each row definition.

Each row explicitly defines every output variable. `STUDYID` and `USUBJID` are
copied from the row-driving dataset, while `RDOMAIN` identifies the SDTM domain
containing the related record and does not depend on a dataset alias.

`RELTYPE` is omitted because this example identifies relationships between
individual records. It is used for `ONE` or `MANY` relationships between
datasets.

The combined values of `STUDYID`, `RDOMAIN`, `USUBJID`, `IDVAR`, `IDVARVAL`,
and `RELID` must be unique and non-missing. The output column order follows the
order in `spec.yaml`.

This fixture covers relationships between individually identified records. A
group-level or dataset-level relationship can require several source records to
collapse into one RELREC row; that case requires a separate row-deduplication
design and is intentionally outside this example.
