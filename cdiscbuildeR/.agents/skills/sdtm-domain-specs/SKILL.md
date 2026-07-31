---
name: sdtm-domain-specs
description: Guidelines for generating SDTM domain YAML configurations in cdiscbuilderR
---
# SDTM Domain Generation Guidelines

When generating a YAML configuration for `cdiscbuilderR`, you must use the appropriate `keys` depending on the SDTM domain class:

## 1. Special Purpose Domains (e.g., DM, CO, SV)
- **Keys**: You MUST explicitly specify `keys: ["StudyOID", "SubjectKey"]` in the YAML configuration if it is a subject-level domain like DM.
- **Granularity**: Often 1 row per subject.
- **Example**:
```yaml
DM:
  - formoid: ["DEMO"]
    keys: ["StudyOID", "SubjectKey"]
    columns:
      USUBJID:
        source: "SubjectKey"
```

## 2. Event Domains (e.g., AE, MH, DS)
- **Keys**: Do NOT specify `keys` unless you have a specific non-standard grouping. The engine will automatically default to `["StudyOID", "SubjectKey", "ItemGroupRepeatKey", "StudyEventOID"]`, which correctly handles repeating forms/logs.
- **Sequence Generation**: You MUST include a sequence generator for the `--SEQ` variable (e.g., `AESEQ`, `MHSEQ`), grouping by `USUBJID`.
- **Example**:
```yaml
AE:
  - formoid: ["AE"]
    columns:
      USUBJID:
        source: "SubjectKey"
      AESTDTC:
        source: "AESTDAT"
      AESEQ:
        group: ["USUBJID"]
        sort_by: ["AESTDTC"]
```

## 3. Finding Domains (e.g., LB, VS, QS)
- **Keys**: Do NOT specify `keys` unless you have a specific non-standard grouping. The engine will default to `["StudyOID", "SubjectKey", "ItemGroupRepeatKey", "StudyEventOID"]`.
- **Sequence Generation**: You MUST include a sequence generator for the `--SEQ` variable (e.g., `LBSEQ`, `VSSEQ`), grouping by `USUBJID`.
- **Core Variables**: Findings must include variables for the test performed (`--TESTCD`, `--TEST`) and the results (`--ORRES`, `--ORRESU`).
- **Example**:
```yaml
LB:
  - formoid: ["LAB"]
    columns:
      USUBJID:
        source: "SubjectKey"
      LBTESTCD:
        source: "LBTCD"
      LBORRES:
        source: "LBRESLT"
      LBSEQ:
        group: ["USUBJID"]
        sort_by: ["LBDTC"]
```

## 4. Reference templates
When asked to build a domain, you can refer to the templates located in `inst/templates/` within the `cdiscbuilderR` package:
- `inst/templates/special_purpose_domain_template.yaml`
- `inst/templates/event_domain_template.yaml`
- `inst/templates/finding_domain_template.yaml`
