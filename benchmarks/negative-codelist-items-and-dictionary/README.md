# Reject a Coding Dictionary That Is Also a List of Terms

**Goal:** build the adverse events domain with each event's
dictionary-derived term (`AEDECOD`), coded with the study's medical
dictionary.

**Input:** collected adverse events, one record per event, coded by the
sponsor, and the study's codelists in the project file.

**Variables:**

- `DOMAIN` is `AE` on every record.
- `AESEQ` would be the collected sequence number of the event.
- `AETERM` would be the event as the investigator reported it.
- `AEDECOD` would be the dictionary term the event was coded to.

**Note:** the codelist both refers to the medical dictionary and lists two of
its terms. A list of terms is the whole set of values a column may hold,
while a dictionary reference defers that set to the dictionary, so the two
contradict each other, and the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Refer to the dictionary and its version alone, since a dictionary is too
large to list:

```yaml
- id: MEDDRA
  name: Medical Dictionary for Regulatory Activities
  external:
    dictionary: MedDRA
    version: "27.1"
```
