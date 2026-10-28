# Parse planned times in a concentration transfer

**Goal:** produce pharmacokinetic concentration (PC) records, retaining
reported results and turning the vendor's nominal sample times into
numeric hours (`PCTPTNUM`).

**Input:** a bioanalytical laboratory's electronic transfer, with sample
identifiers, subject identifiers, nominal time labels, analyte, reported
plasma concentration, units and actual collection date and time.

**Variables:**

- `PCSEQ` numbers samples within each subject by actual collection time,
  then sample identifier; `PCREFID` retains that identifier.
- `PCTESTCD` and `PCTEST` identify the measured drug, `ZX201` and `ZX-201`.
- `PCORRES` and `PCSTRESC` retain the reported result as text, including
  `BLQ` for below the limit of quantification.
- `PCORRESU` and `PCSTRESU` retain the supplied unit independently of the
  result. They are empty when no unit is supplied; no unit conversion is
  applied.
- `PCSTRESN` holds numeric results and is empty for `BLQ`; no limit-based
  numeric replacement is made.
- `PCSPEC` is `PLASMA`, and `PCDTC` retains the actual collection time.
- `PCTPT` retains the vendor's nominal label. `PCTPTNUM` is zero for
  `PRE`, the number before `H` for planned post-dose samples, and empty
  for `UNSCHED`.

**Note:** planned hours come from the nominal label, regardless of the
actual collection time. An unrecognized label stops the run.
