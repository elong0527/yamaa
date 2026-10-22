# Reject an ODM 1.2 Export

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** an export from an older electronic data capture (EDC) system
written as an Operational Data Model (ODM) 1.2 XML file, whose root element
names the ODM 1.2 namespace.

**Variables:**

- `SEX` would be the subject's collected sex code.

Only ODM 1.3 and 2.0 documents are read. ODM 1.2 is an earlier version of
the standard, and reading its elements as if they followed 1.3 could give a
collected value a meaning the export never gave it. The input is rejected
while it is read, and no artifact is accepted.

## How to fix

Export the clinical data as ODM 1.3 or 2.0. The root element of such an
export names the matching namespace and version:

```xml
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3.2"
     FileType="Snapshot">
```

Do not edit the namespace of a 1.2 file by hand; the version is a property
of how the file was written, not a label.
