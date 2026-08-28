# SDTM AE effective transaction selection

This focused probe answers one question: given a transactional collected
source, can the effective state of a record be selected?

## Rule and input boundary

Two inputs are used. `AE_REC` is the inventory of adverse-event records and is
the row driver, so each record produces one AE row. `AE_TXN` is the
transaction log, holding an insert and one later change for each record.

Every value column selects the transaction with the greatest audit timestamp,
breaking ties on `TXNSEQ`. The four records cover a normal correction, a
removal, an audit timestamp that disagrees with document order, and two
transactions bearing the same timestamp.

## What the fixture shows

**Selection works.** Ordered `source.multiple_matches` is enough to take the
last transaction per record, and the expected `multiple_matches` count is four
on each of the four value columns.

**Removal does not.** Record `CATH-UCSD-0001/2` was removed. Its effective
state is that the record does not exist, and the committed output still
contains it, carrying the values from the removing transaction and
`TXNTYPE = REMOVE`. Row filters run during row construction and can reference
only the row driver, so the inventory row cannot be filtered by a value that is
resolved later from another dataset. Nothing in the column phase can delete a
row. The golden file therefore commits a record that must not be in a final AE
dataset, which is the finding rather than an oversight.

**Audit order and document order can disagree.** For `CATH-UCSD-0002/1` the
second transaction in document order carries the earlier timestamp. Ordering by
timestamp keeps the insert and reports `MILD`; ordering by document position
would keep the update and report `SEVERE`. The fixture commits the
timestamp answer because that is what the specification asks for, and the
disagreement is not detectable from the output.

**Equal timestamps need a second key.** For `CATH-UCSD-0002/2` both
transactions share a timestamp, and only `TXNSEQ` makes the selection total.
Without it the result would be ambiguous, which R008 requires the specification
to prevent rather than the runtime to guess.

## Three further gaps

**Each column selects independently.** Four separate `multiple_matches`
selections agree here only because all four declare the same ordering. Nothing
ties them to one transaction record, the same gap
`../sdtm-dm-reference-dates` records for aggregates.

**There is no datetime type.** R011 declares none, so `AUDITDTC` is carried as
`str` and ordering is correct only because ISO 8601 text sorts chronologically.

**Provenance is not modeled.** Transaction type and audit timestamp survive
only because this fixture emits them as columns. Nothing states what provenance
a derived record must carry.

Whether transactional ODM is in scope at all is the decision this fixture is
meant to force. If only snapshots are in scope, the removal gap disappears and
these three remain.

## Diagnostics and verifications

Expected `multiple_matches` count is four for each of `AETERM`, `AESEV`,
`TXNTYPE`, and `AUDITDTC`. No other handler path is declared.

Rows remain in `AE_REC` order; the key is `[STUDYID, USUBJID, AESEQ]`; exactly
four rows are expected, including the removed one.
