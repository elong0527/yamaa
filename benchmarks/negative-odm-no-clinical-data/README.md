# Reject an ODM Export Without Collected Data

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** an Operational Data Model (ODM) 1.3 XML file from the electronic
data capture (EDC) system that describes the study's visits, forms, items
and code lists but holds no collected data: an export of the study design,
like a define.xml, instead of the clinical data export.

**Variables:**

- `SEX` would be the subject's collected sex code.

The file has no clinical data section, so it holds no subject at all. A
listing built from it would be empty and would look like a study that has
enrolled no one, so the input is rejected once the whole file has been read,
and no artifact is accepted.

## How to fix

Export the clinical data from the EDC system: an ODM snapshot whose
`ClinicalData` element holds the subjects' collected items. Keep the study
design export for the define.xml and other metadata work, and save the
clinical data export where the input reads it:

```yaml
input:
  ODM: input/odm.xml
```

A clinical data export with no subjects yet still has its `ClinicalData`
element, and it gives an empty listing rather than this rejection.
