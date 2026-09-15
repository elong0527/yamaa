# Record findings about an adverse event and link them to it

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-fa-event-findings.html)

**Goal:** show how findings collected on a supplementary form about an
individual adverse event map to FA, linked to AE via `FALNKID` and
RELREC.

**Input:** `input/odm.csv` (ODM-style extract: AE form, rash
supplementary-findings form, CM form; 2 subjects, one with two rash
events).

**Variables:**

- AE carries `AETERM`, `AESTDTC`, and `AELNKID`; each rash event gets a
  link identifier that the FA and RELREC records share.
- CM carries `CMSEQ`, `CMTRT`, and `CMSTDTC`; it is included as context
  for the collected medications.
- `FAOBJ` is the finding object, fixed to RASH because every
  supplementary record describes a rash event.
- `FASEQ` numbers each subject's FA records.
- `FACAT` and `FASCAT` classify the record as a skin-finding
  supplement.
- `FALNKID` carries the link identifier that ties each FA finding
  back to one AE row.
- `FATESTCD` and `FATEST` name the collected measurement (location,
  diameter, or biopsy status).
- `FAORRES`, `FAORRESU`, `FASTRESC`, and `FASTRESU` hold the result
  in original and standard form; units are supplied for diameter
  only.
- RELREC records the one-to-many relationship between AE and FA
  using `RDOMAIN`, `IDVAR`, `IDVARVAL`, `RELTYPE`, and `RELID`.

**Note:** all terms are invented for illustration; this is not real
patient data.

**Standard:** SDTM | **Domains:** AE, CM, FA, RELREC
