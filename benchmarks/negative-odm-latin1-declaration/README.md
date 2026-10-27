# Reject an ODM Export Declared in Latin-1

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file whose first line declares the
Latin-1 (ISO-8859-1) character encoding instead of UTF-8.

**Variables:**

- `SEX` would be the subject's collected sex code.

The same bytes spell different accented letters in Latin-1 and in UTF-8, so
a site or investigator name read in the wrong encoding comes out as a
different name. Only UTF-8 is read, or plain ASCII that declares itself
US-ASCII, and the file states that it is Latin-1, so the input is rejected
while it is read, and no artifact is accepted.

## How to fix

Export the file again from the EDC system as UTF-8, so that its first line
reads:

```xml
<?xml version="1.0" encoding="UTF-8"?>
```

Do not change only that line: the bytes of every accented letter would stay
in Latin-1 and be read as other letters.
