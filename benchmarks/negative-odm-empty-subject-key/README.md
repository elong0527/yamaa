# Reject a Subject Without a Subject Key

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, in which one subject's
`SubjectKey`, the subject's identifier within the study, is empty.

**Variables:**

- `SEX` would be the subject's collected sex code.

The subject key builds the unique subject identifier (`USUBJID`). The
second subject's answers would belong to no identifiable subject, and a
record built under an empty key could not be joined to any other dataset of
the study. The input is rejected while it is read, and no artifact is
accepted.

## How to fix

Find the subject in the EDC system, make sure its subject key is recorded,
and export again:

```xml
<SubjectData SubjectKey="102">
```

Do not fill in a key by hand in the export; the key must be the one the EDC
system holds for the subject.
