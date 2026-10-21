# Reject a Transactional ODM Export

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file, saved as a transactional file: a
log of every entry, change and removal in the order they were made, instead
of the current answers.

**Variables:**

- `SEX` would be the subject's collected sex code, as the form holds it now.

A transactional file lists an answer once for each time it was entered or
changed, and lists a removed answer as well as its removal, so its records
are not the subjects' current answers. Read as if they were, a corrected
answer would count twice and a removed form would still give an answer. The
input is rejected while it is read, and no artifact is accepted.

## How to fix

Export a snapshot from the EDC system instead: the current value of every
collected item, which the export marks with `FileType="Snapshot"` on its
root `ODM` element, or with no `FileType` at all. Applying the transactions
in order is the EDC system's job, so do not edit the transactional file by
hand to make it look like a snapshot:

```yaml
input:
  ODM: input/odm.xml
```

The specification itself does not change; only the export does.
