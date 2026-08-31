# ADaM ADTR: reject a repeated scheduled assessment

This example uses a tumour-assessment schedule to attempt one row per subject
and analysis visit:

- `AVISITN` is the planned order of the assessment;
- `ADT` is its scheduled analysis date.

The same subject and analysis visit appears twice. The intermediate assessment
grain therefore has no unique row to offer later calculations, and the run
must fail before an artifact is constructed.

## How to fix

First decide whether the two schedule rows are duplicates or distinct
assessments. Remove the unsupported duplicate when they describe the same
visit. If the protocol genuinely schedules two assessments with the same
label, give them distinct analysis-visit identities and include that declared
identity in the keys:

```yaml
keys: [STUDYID, USUBJID, AVISIT, ASEQ]
```

Do not keep one duplicate by source order. That would make the chosen schedule
record depend on storage rather than the protocol.
