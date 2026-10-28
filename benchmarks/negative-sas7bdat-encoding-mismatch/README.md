# Reject a text encoding that contradicts the received file

**Goal:** carry the record identifier (`ID`) and preserve the concentration
unit (`UNIT`) exactly as received.

**Input:** a SAS dataset storing its text as UTF-8, with a micro sign in
its concentration unit. The requested decoding instead assumes Windows-1252.

**Variables:**

- `UNIT` would hold the received concentration unit. The contradictory
  text encoding is refused before any output value is accepted.

**Note:** the file records how its text is encoded. Choosing another
encoding would change a micro sign into different characters.

## How to fix

Honor the encoding recorded by the sender. Omit the contradictory
encoding declaration so the file's own encoding supplies the decoding:

```yaml
input:
  SRC:
    path: input/concentration.sas7bdat
    types: {ID: int}
```

An explicit `encoding: utf-8` agrees with this file too. A transport file
stores no encoding, so its declaration must still be supplied.
