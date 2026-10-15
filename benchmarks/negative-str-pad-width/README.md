# Reject a Nonnumeric Padding Width

**Goal:** format a numeric laboratory sequence as an eight-character key.

**Input:** one laboratory record with a numeric sequence.

**Variables:**

- `IDVARVAL` would be the sequence number padded to eight characters. The
  written width is text, so this output is rejected before data ingestion.

## How to fix

Set the padding width to the number of characters required:

```yaml
str_pad: {source: LB.LBSEQ, width: 8}
```
