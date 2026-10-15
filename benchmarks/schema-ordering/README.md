# Ordering

**Goal:** fix the order of the output rows: two sort terms, a
placement for missing values set apart from the sort direction, and a
stable order for ties.

**Input:** one `spec.yaml` over one adverse-event dataset `AE`,
stored out of order, carrying subject, sequence number, decoded term
(`AEDECOD`, copied through unchanged), and analysis start day
(`AESTDY`), which may be missing.

**Order:**

- first by analysis start day (`AESTDY`), ascending, with missing
  days placed before every present day;
- then, among rows on the same day or both missing a day, by
  sequence number (`AESEQ`), descending.

**Note:** missing values go where the order places them, first or
last, whatever its direction, so an ascending sort can still put
missing days first. Rows equal on both terms keep their input order.
