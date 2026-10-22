# Refuse a Data Definition Without the Annotated Case Report Form

**Goal:** submit the demographics domain with its data definition, the sex
(`SEX`) traced to the case report form it was collected on.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SEX` is the sex as the investigator collected it.

**Note:** the sex was collected by the investigator, so the data definition
must point a reviewer to the page of the annotated case report form that
collected it. The study declares no annotated case report form, so the
submission is refused before any document is written; the dataset itself
still builds.

## How to fix

Declare the study's annotated case report form in the section, and point the
collected value at its page:

```yaml
sdtm:
  documents:
    - id: ACRF
      kind: annotated_crf
      link: acrf.pdf
      title: Annotated Case Report Form
```

```yaml
origins:
  - type: Collected
    source: Investigator
    documents:
      - document: ACRF
        pages: {type: PhysicalRef, refs: "3"}
```
