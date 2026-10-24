# Reject a Comment Piece Counted from Zero

**Goal:** keep the first 200 characters of each collected comment in
`COVAL`.

**Input:** collected free-text comments, one record per comment, with the
subject and the comment number (`COSEQ`).

**Variables:**

- `COVAL` would be the first 200 characters of the comment, but the piece
  is written as starting at character 0, and character positions count
  from 1.

The request is rejected before any data is read, and no artifact is
accepted.

## How to fix

Start the piece at the comment's first character, which is position 1:

```yaml
str_substr: {source: CO_RAW.COMMENT, start: 1, length: 200}
```
