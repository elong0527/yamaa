# Refuse a Data Definition of a Variable Without Its Metadata

**Goal:** submit the demographics domain with its data definition, including
each subject's study site (`SITEID`).

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SITEID` is the identifier of the subject's study site.

**Note:** the study site was added to the domain late, with a label but
without the metadata a data definition describes a variable by: its core
designation, role, length and origin. The document cannot describe it, so
the submission is refused before any document is written; the dataset
itself still builds.

## How to fix

Give the variable its submission metadata:

```yaml
- name: SITEID
  type: str
  label: Study Site Identifier
  derivation: RAW.SITEID
  submission:
    core: Req
    role: Record Qualifier
    length: 3
    origins:
      - type: Assigned
        source: Sponsor
```
