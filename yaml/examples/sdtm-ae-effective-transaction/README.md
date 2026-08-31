# SDTM AE: take the effective state of a record from a transaction log

This example uses an inventory of adverse-event records with the transaction
log that amends them, and a `yamaa` specification to derive one record per
effective adverse event:

- `AETERM` and `AESEV` are the reported term and severity as they stand after
  the last change, not as first entered;
- `TXNTYPE` and `AUDITDTC` are the kind of that last change and when it was
  made.

The last change is the one with the latest audit timestamp; when two carry the
same timestamp the higher transaction sequence is later. Ordering by timestamp
rather than by position in the log is deliberate, and the two can disagree: a
record whose second logged transaction was stamped earlier keeps the values
from the first.

A record whose last transaction removed it does not appear. The removal
decision is made on the completed effective state before the final AE rows are
constructed.
