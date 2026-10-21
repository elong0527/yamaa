# Reject an Analysis Guide Cited for the Tabulation Datasets

**Goal:** build the demographics dataset, one record per subject, and submit
it with its data-definition document.

**Input:** the study's subjects, one row each.

**Variables:**

- `USUBJID` identifies each subject across the study.

**Note:** tabulation and analysis datasets are submitted with separate data
definitions, each citing the implementation guides of one CDISC model. The
study cites the analysis supplement for medical devices among the guides of
its tabulation datasets, so the submission stops before any file is written;
the dataset itself still builds.

## How to fix

Cite the analysis supplement with the analysis datasets, and the tabulation
supplement for medical devices with the tabulation datasets, if the study
submits device data:

```yaml
sdtm:
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}
    - {name: SDTMIG-MD, version: "1.1", status: Final}
adam:
  standards:
    - {name: ADaMIG, version: "1.3", status: Final}
    - {name: ADaMIG-MD, version: "1.0", status: Final}
```

Otherwise remove the analysis supplement from the tabulation section.
