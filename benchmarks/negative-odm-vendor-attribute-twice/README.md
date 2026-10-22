# Reject a Study Subject Number Given by Two Vendors

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file. Two vendor extensions, one from
the EDC system and one from the randomization system, each give the second
subject a study subject number: `S-102` and `R-0007`.

**Variables:**

- `SEX` would be the subject's collected sex code.

The listing of collected items keeps one study subject number for each
subject. The export gives two, from two vendors, and says nothing about
which one the study uses, so keeping either would hide the other. The input
is rejected while it is read, and no artifact is accepted.

## How to fix

Decide which system owns the study subject number and export only its
attribute, for example by leaving the randomization extension out of the
EDC export:

```xml
<SubjectData SubjectKey="102" edc:StudySubjectID="S-102">
```
