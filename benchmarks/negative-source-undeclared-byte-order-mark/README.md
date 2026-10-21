# Reject an Undeclared Byte-Order Mark

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`, saved by a spreadsheet
program as UTF-8 comma-separated text whose lines end in a carriage return
and a line feed.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled at, taken from
  the collected value.

The file starts with a byte-order mark, the invisible character U+FEFF that
some programs write before UTF-8 text, and nothing the study states says
that it does. Kept, the mark would sit in front of the first field name;
dropped, the reader would have decided on its own what the file holds. So the
run is rejected while the file is read, and no artifact is accepted.

## How to fix

Confirm that the program that exports the listing always starts its files
with a byte-order mark, then state it on the input, so that exactly that one
mark is skipped:

```yaml
input:
  DM:
    path: input/dm.csv
    byte_order_mark: true
```

Do not drop the mark by saving the listing again in a spreadsheet program's
plain comma-separated format: that format writes the computer's own character
encoding, so the accented letter in the second site name can change. Removing
the mark with a tool that keeps the text UTF-8 is safe, and then the input
states nothing.
