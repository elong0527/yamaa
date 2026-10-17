# Ordering

**Goal:** fix the order of the output rows: two sort terms, missing
values sorting as the lowest value, and a stable order for ties.

**Input:** one `spec.yaml` over one adverse-event dataset `AE`,
stored out of order, carrying subject, sequence number, decoded term
(`AEDECOD`, copied through unchanged), and analysis start day
(`AESTDY`), which may be missing.

**Order:**

- first by analysis start day (`AESTDY`), ascending, so missing days
  come before every present day;
- then, among rows on the same day or both missing a day, by
  sequence number (`AESEQ`), descending.

**Note:** a missing value sorts as lower than every present value:
first in an ascending sort and last in a descending one, unless the
order states where missing values go. Rows equal on both terms keep
their input order.
