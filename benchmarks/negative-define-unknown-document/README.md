# Refuse a Data Definition That Points to an Undeclared Document

**Goal:** submit the demographics domain with its data definition, the study
identifier traced to the protocol that fixes it.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.

**Note:** the study identifier points a reviewer to a document named
`PROTOCOL`, but the study declares its protocol under the name `PROT`, and no
document as `PROTOCOL`. The data definition would point to a document the
package does not hold, so the submission is refused before any document is
written; the dataset itself still builds.

## How to fix

Point the reference at the document the section declares:

```yaml
origins:
  - type: Protocol
    source: Sponsor
    documents: [PROT]
```
