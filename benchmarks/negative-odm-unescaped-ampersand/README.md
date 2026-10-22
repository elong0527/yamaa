# Reject an Unescaped Ampersand in an ODM Export

**Goal:** build one Demographics (DM) record per subject with the name of
the investigator (`INVNAM`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, in which one investigator name,
`Drs Lind & Berg`, holds a bare ampersand.

**Variables:**

- `INVNAM` would be the name of the investigator responsible for the
  subject, as the form holds it.

In XML an ampersand starts an escape, such as `&amp;` for the ampersand
itself, so a bare one leaves the file malformed. What the name says, and
where the value ends, would then depend on how a reader chose to repair the
file. The input is rejected while it is read, and no artifact is accepted.

## How to fix

Export the file again from the EDC system, whose XML writer escapes an
ampersand in a value as `&amp;`:

```xml
<ItemData ItemOID="IT.DM.INVNAM" Value="Drs Lind &amp; Berg"/>
```

Do not repair the export by hand; if the EDC system wrote the bare
ampersand itself, report the defect to its vendor.
