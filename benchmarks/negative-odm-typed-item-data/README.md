# Reject Typed Item Elements in an ODM Export

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, written with the typed item
elements of ODM 1.3, such as `ItemDataString`, instead of plain items that
carry their answer in `Value`.

**Variables:**

- `SEX` would be the subject's collected sex code.

A typed element states the kind of its answer in its own name, which the
listing of collected items has no place to keep. Reading the answer as
plain text would silently drop what the export says about it, so the input
is rejected while it is read, and no artifact is accepted.

## How to fix

Export the clinical data with plain items, which most EDC systems write by
default for ODM 1.3:

```xml
<ItemData ItemOID="IT.DM.SEX" Value="F"/>
```
