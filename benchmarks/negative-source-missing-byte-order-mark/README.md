# Reject a Missing Byte-Order Mark

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`, which the study expects
from a program that starts its files with a byte-order mark.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled at, taken from
  the collected value.

The study states that the listing starts with a byte-order mark, the
invisible character U+FEFF that some programs write before UTF-8 text, but
the file starts directly with its header. A file that differs from what the
study states may have come from another program or another export than the
one the study expects, so the run is rejected while the file is read, and no
artifact is accepted.

## How to fix

Find out which program wrote the listing. If it should come from the program
that writes the mark, export it from there again rather than changing the
study. If the study now receives the listing from a source that writes no
mark, stop stating one:

```yaml
input:
  DM: input/dm.csv
```
