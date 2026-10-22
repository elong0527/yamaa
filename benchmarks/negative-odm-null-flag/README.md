# Reject an Unknown Null Marker in an ODM Export

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, in which the second subject's
sex is marked as not collected with `IsNull="Y"`.

**Variables:**

- `SEX` would be the subject's collected sex code.

ODM marks an item that holds no answer with `IsNull="Yes"`. The reader
admits `yes`, `true` or `1` for a null item and `no`, `false` or `0` for an
answered one, in any letter case, and `Y` is none of them. Reading it either
way would decide on the export's behalf whether the answer is missing, so the
input is rejected while it is read, and no artifact is accepted.

## How to fix

Have the EDC system write the null marker as ODM defines it:

```xml
<ItemData ItemOID="IT.DM.SEX" IsNull="Yes"/>
```

If the system cannot be changed, ask its vendor which value `Y` stands for
before the export is used.
