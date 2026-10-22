# Refuse a Copied Age Labelled Differently From Its Source

**Goal:** submit the subject-level analysis dataset with its data
definition, the age (`AGE`) copied unchanged from demographics.

**Input:** the demographics domain, one record per subject, whose
specification `spec_dm.yaml` the study's tabulation section lists.

**Variables:**

- `AGE` is the age in years, copied from the demographics domain.

**Note:** the age is copied unchanged and keeps its name, but its label,
`Age at Informed Consent`, differs from the demographics label `Age`. One
variable copied under one name would carry two descriptions across the
package, so the submission is refused before any document is written; the
dataset itself still builds.

## How to fix

Keep the label of the variable the value was copied from:

```yaml
- name: AGE
  type: int
  label: Age
  derivation: DM.AGE
```

When the analysis value means something else, give it a name of its own,
which may then carry its own label.
