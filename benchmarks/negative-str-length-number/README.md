# Reject the Length of a Comment Number

**Goal:** keep each collected comment in `COVAL`, and stop the run for a
comment longer than the 200 characters one value holds.

**Input:** collected free-text comments, one record per comment, with the
subject and the comment number (`COSEQ`).

**Variables:**

- `COVAL` is the comment as collected. Its length is counted from the
  comment number instead of the comment text, and a number has no
  characters to count.

The request is rejected before any data is read, and no artifact is
accepted.

## How to fix

Count the characters of the comment text, not of its number:

```yaml
str_length: {source: CO_RAW.COMMENT}
```
