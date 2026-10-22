# Reject a Dataset Label Kept in Free-Form Notes

**Goal:** build the demographics domain, with the metadata a data definition
describes it by and free-form notes for the study team.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.

**Note:** the free-form notes carry a `label`, the name of a governed
dataset field. Notes are never read, so a label kept there would sit beside
the submitted label without changing it, and a reader could take the wrong
one for the dataset's label; the run is rejected before any data is read and
no artifact is accepted.

## How to fix

Keep the dataset label only where it is submitted, and give the note another
name:

```yaml
metadata:
  owner: Data management
  draft_label: Demographics (draft)
```
