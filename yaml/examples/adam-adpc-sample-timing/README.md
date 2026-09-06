# ADaM ADPC: derive sample timing

The collected pharmacokinetic samples produce one analysis row per specimen:

- `PCSEQ` preserves the collected sequence that identifies the specimen.
- `PCDT` is the collected specimen date.
- `PCTM` is the collected local time of day, written with seconds even when
  seconds were omitted in the source.
- `REFDATM` is the complete local reading from which elapsed seconds are
  measured.
- `ADTM` combines the collected date and local time; it is missing when either
  component is missing.
- `ELTM` is the signed number of whole seconds from the reference reading to
  `ADTM`. It crosses midnight by using both dates, is negative for an earlier
  sample, and is missing when either reading is missing.
