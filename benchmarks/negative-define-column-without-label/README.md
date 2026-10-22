# Refuse a Data Definition of a Variable Without a Label

**Goal:** submit the demographics domain with its data definition, including
each subject's study site (`SITEID`).

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SITEID` is the identifier of the subject's study site.

**Note:** the study site was added to the domain late and given no label. A
data definition describes every variable it lists with a label, so it cannot
describe this one, and the submission is refused before any document is
written; the dataset itself still builds.

## How to fix

Give the variable its label:

```yaml
- name: SITEID
  type: str
  label: Study Site Identifier
  derivation: RAW.SITEID
```
