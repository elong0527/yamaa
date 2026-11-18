# One Dose Record per Counted Administration

**Goal:** give one row for each administration a collected exposure (EX)
record counts, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE`
and `EXDOSU` through, numbering each administration in `ADOSEN` beside the
record's count in `NDOSE`.

**Input:** one `spec.yaml` over collected exposure records, each with its
treatment, dose, unit and the number of administrations it stands for
(`EXDOSCNT`).

**Variables:**

- `EXDOSE` is the amount given at each administration.
- `ADOSEN` numbers the administrations built from one collected record from
  `1` up to its count.
- `NDOSE` is how many administrations the collected record stands for,
  taken from `EXDOSCNT`.

**Note:** a collected record builds as many rows as it counts, whatever the
count, and a record that counts no administration builds none.
