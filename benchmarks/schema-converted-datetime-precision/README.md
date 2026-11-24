# Collected Precision of a Converted Analysis Start

**Goal:** one row per adverse event (AE) with an analysis start date/time
(`ASTDTM`) read directly from a collected start that carries at least its
minutes, how much of it was collected (`ASTDTPR`), and its time imputation
flag (`ASTTMF`).

**Input:** one `spec.yaml` over adverse event records carrying the reported
term (`AETERM`) and the collected start (`AESTDTC`), collected to the minute
or to the second.

**Variables:**

- `ASTDTM` is the analysis start date/time; a start collected only to the
  minute takes `00` seconds.
- `ASTDTPR` is `minute` when the start stopped at its minutes and `second`
  when it carried its seconds, including seconds collected as `00`.
- `ASTTMF` is `S` when only the seconds were supplied; empty when the time
  was collected to the second.

**Note:** a start read directly into a date/time keeps how much of it was
collected, exactly as one completed by imputation does, so both ways of
making `ASTDTM` give the same precision and the same flag.
