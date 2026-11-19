# Reject a Key on a Counted Dose Relation

**Goal:** give one row for each administration a collected exposure (EX)
record counts, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE`
and `EXDOSU` through, numbering each administration in `ADOSEN` beside the
record's count in `NDOSE`.

**Input:** one `spec.yaml` over collected exposure records, each with its
treatment, dose, unit and the number of administrations it stands for
(`EXDOSCNT`).

**Variables:**

- `ADOSEN` numbers the administrations built from one collected record from
  `1` up to its count.
- `NDOSE` is how many administrations the collected record stands for,
  taken from `EXDOSCNT`.

**Note:** the administrations of a record are built from that record
alone, so nothing is matched to it, and a declaration that also names
what to match is refused before any record is read.

## How to fix

Remove `key` from the relation that writes `repeat`: its records are the
records of its `dataset`, each once for every number up to its count, and
no other record is matched to them.

```yaml
intermediates:
  - id: ADMIN
    dataset: EX
    repeat: {count: EX.EXDOSCNT, index: N}
    derivations:
      STUDYID: EX.STUDYID
      USUBJID: EX.USUBJID
      EXSEQ: EX.EXSEQ
      EXTRT: EX.EXTRT
      EXDOSE: EX.EXDOSE
      EXDOSU: EX.EXDOSU
      ADOSEN: N
      NDOSE: EX.EXDOSCNT
```

To read the administrations by subject from another place, declare the
key there: a lookup or an aggregate over `ADMIN` states a `key` of its
own.
