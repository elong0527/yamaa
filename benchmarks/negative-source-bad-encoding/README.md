# Reject Bad Encoding

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled
  at, taken from the collected value.

One stored site name holds a byte written in an older encoding,
which is not valid UTF-8 text. Replacing the byte with a
substitute character would store a name the study never recorded,
and dropping it would store a different name again, so the run is
rejected while the file is read and no artifact is accepted.

## How to fix

Export the listing as UTF-8, so every collected character keeps the value it
was entered with. The second site name then stores its accented letter (o
with circumflex, U+00F4) as the two bytes `C3 B4` instead of the single byte
`F4` that the older encoding wrote.

Converting a file after the fact is safe only while the original encoding is
known. A study that stores names in an unstated encoding cannot say which
letters it collected.
