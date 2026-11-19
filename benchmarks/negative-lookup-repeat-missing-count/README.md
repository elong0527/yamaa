# Reject a Dose Record with No Count

**Goal:** give one row for each administration a collected exposure (EX)
record counts, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE`
and `EXDOSU` through, numbering each administration in `ADOSEN` beside the
record's count in `NDOSE`.

**Input:** one `spec.yaml` over collected exposure records, each with its
treatment, dose, unit and the number of administrations it stands for
(`EXDOSCNT`), one of them with no count.

**Variables:**

- `ADOSEN` numbers the administrations built from one collected record from
  `1` up to its count.
- `NDOSE` is how many administrations the collected record stands for,
  taken from `EXDOSCNT`.

**Note:** a collected record with no count cannot say how many
administrations it stands for, so it rejects the run, and no artifact is
accepted.

## How to fix

Decide what a record with no count stands for, and say it. When it should
be collected, correct the exposure data. When such a record stands for
no administration, leave it out before the count is read:

```yaml
intermediates:
  - id: COUNTED
    dataset: EX
    filter: "EX.EXDOSCNT IS NOT NULL"
  - id: ADMIN
    dataset: COUNTED
    repeat: {count: COUNTED.EXDOSCNT, index: N}
    derivations:
      STUDYID: COUNTED.STUDYID
      USUBJID: COUNTED.USUBJID
      EXSEQ: COUNTED.EXSEQ
      EXTRT: COUNTED.EXTRT
      EXDOSE: COUNTED.EXDOSE
      EXDOSU: COUNTED.EXDOSU
      ADOSEN: N
      NDOSE: COUNTED.EXDOSCNT
```

When it stands for a single administration, give it that count with
`first_available` in a derivation of `COUNTED` and count from the derived
field. Do not read a missing count as zero without deciding so: the
record would then leave the dataset without a word.
