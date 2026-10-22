# Reject an Item That Is Null and Answered

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, in which the second subject's
sex is marked as not collected (`IsNull="Yes"`) and still carries the value
`F`.

**Variables:**

- `SEX` would be the subject's collected sex code.

An item is either missing or answered. One that says both has no single
answer: keeping `F` ignores the mark, and keeping the item missing drops a
recorded value. The input is rejected while it is read, and no artifact is
accepted.

## How to fix

Check the subject's form in the EDC system. If the answer was collected,
clear the null mark; if it was not, remove the value. Then export again:

```xml
<ItemData ItemOID="IT.DM.SEX" Value="F"/>
```
