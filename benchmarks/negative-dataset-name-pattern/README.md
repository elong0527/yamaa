# Reject a Dataset Name With a Hyphen

**Goal:** keep the questionnaire responses of one instrument as a dataset of
their own, named `QS-PHQ9`.

**Input:** collected questionnaire responses, one record per subject and
question.

**Variables:**

- `DOMAIN` is `QS` on every record.
- `QSSEQ` would be the collected sequence number of the response.
- `QSTESTCD` would be the short name of the question as collected.
- `QSORRES` would be the response as collected.

**Note:** a dataset name holds only letters, digits and underscores, and
does not start with a digit, so that every reader and every transport format
can name it. `QS-PHQ9` holds a hyphen, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Name the dataset with letters, digits and underscores only, and keep the
instrument in the name without the hyphen:

```yaml
domain: QSPH
```
