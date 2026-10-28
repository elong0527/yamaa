# Preserve concentration units using the received text encoding

**Goal:** carry the record identifier (`ID`) and preserve the concentration
unit (`UNIT`) exactly as received.

**Input:** a specification reads a SAS dataset whose header records UTF-8;
no text encoding is declared by the specification.

**Variables:**

- `UNIT` is the concentration unit as received, preserving its micro sign.

**Note:** the file's stored text encoding supplies the decoding when none
is declared. A declaration, when present, must agree with the file.
