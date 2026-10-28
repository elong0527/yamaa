# Reject a Padded Key Longer Than One Text Value May Hold

**Goal:** format a numeric laboratory sequence as a key padded to 1,048,577
characters.

**Input:** one laboratory record with a numeric sequence.

**Variables:**

- `IDVARVAL` would be the sequence number with leading spaces to 1,048,577
  characters. Padded text may hold at most 1 MiB, 1,048,576 bytes, so the run
  is refused before the value is built and no dataset is written.

## How to fix

Pad the key to the width its use needs. A SAS transport character value holds
at most 200 bytes, so a key padded for one fits well within the limit:

```yaml
str_pad: {source: LB.LBSEQ, width: 8}
```
