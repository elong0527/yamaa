# `sdtm-ae-dictionary-coding` as Excel sheets

`spec.yaml` written in the sheets of a typical Excel specification, as in
[Excel to YAMAA](../../../docs/excel-to-yamaa.md), section 1. Every cell comes
from `spec.yaml`; an empty cell is one the specification does not state.

**Dataset sheet**

| Dataset | Description | Class | Structure | Key Variables |
|---|---|---|---|---|
| AE | | | One record per AE_RAW record | STUDYID, USUBJID, AESEQ |

**Variable sheet**

| Variable Name | Variable Label | Type | Length | Controlled Terms or Format | Origin | Core | Conversion Definition | Variable Type | Variable Order | Comments for Define |
|---|---|---|---|---|---|---|---|---|---|---|
| DOMAIN | Domain Abbreviation | Char | | | Assigned | | "AE" | AE | 1 | |
| STUDYID | Study Identifier | Char | | | Collected | | AE_RAW.STUDYID | AE | 2 | |
| USUBJID | Unique Subject Identifier | Char | | | Collected | | AE_RAW.USUBJID | AE | 3 | |
| AESEQ | Sequence Number | Num | | | Collected | | AE_RAW.AESEQ | AE | 4 | |
| AETERM | Reported Term for the Adverse Event | Char | | | Collected | | AE_RAW.AETERM | AE | 5 | |
| AEDECOD | Dictionary-Derived Term | Char | | MedDRA 26.1 | Assigned | | MEDDRA.PTNAME where MEDDRA.LLTNAME = AETERM; "NOT CODED" when AETERM is empty or not found | AE | 6 | |
| AEBODSYS | Body System or Organ Class | Char | | MedDRA 26.1 | Assigned | | MEDDRA.SOCNAME, same match and fallback | AE | 7 | |
