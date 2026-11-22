# Name a Study's Codelists by Dataset and Variable

**Goal:** carry the planned arm code (`ARMCD`) into ADSL and give its
description (`ARM`), with a data-definition document listing the coded values
of each under a codelist named for its dataset and variable.

**Input:** `spec.yaml` reads one pre-analysis row per subject, and
`project.yaml` declares the study's two codelists, named `ADSL.ARMCD` and
`ADSL.ARM`.

**Variables:**

- `ARMCD` is the planned arm code, one of the coded values of `ADSL.ARMCD`.
- `ARM` is the description of the planned arm: the decode `ADSL.ARMCD` gives
  the subject's arm code, one of the values of `ADSL.ARM`.

**Note:** a codelist's name may join several parts with periods, as submitted
data-definition documents commonly name a codelist for the dataset and
variable it serves. The document identifies each codelist by that name behind
a fixed `CL.` prefix, so the arm code's codelist is `CL.ADSL.ARMCD`.
