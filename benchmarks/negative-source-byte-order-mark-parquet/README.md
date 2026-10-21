# Reject a Byte-Order Mark on a Parquet Input

**Goal:** carry the site identifier (`SITEID`) onto a subject listing.

**Input:** collected demographics stored as a Parquet file, carrying
`SITEID`.

**Variables:**

- `SITEID` would be the site the subject enrolled at, as collected.

The study states that the input starts with a byte-order mark, the invisible
character U+FEFF that some programs write before UTF-8 comma-separated text.
A Parquet file is not text and never starts with one, so the statement
cannot hold, and the run is rejected before any data is read.

## How to fix

Stop stating a byte-order mark on the Parquet input; Parquet stores its text
in one fixed encoding of its own:

```yaml
input:
  DM: input/dm.parquet
```

If the study meant to read a comma-separated export of the same data, name
that file instead, and state the mark only when the export starts with one.
