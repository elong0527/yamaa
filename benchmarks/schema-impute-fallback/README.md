# Analysis Start Date/Time Falling Back to First Exposure

**Goal:** one row per adverse event (AE) with an analysis start date/time
(`ASTDTM`) completed from the collected start, or taken from first exposure
when no start was collected, and the date and time imputation flags
(`ASTDTF`, `ASTTMF`).

**Input:** one `spec.yaml` over adverse event records carrying the
reported term (`AETERM`) and the collected start (`AESTDTC`), which may be a
date and time, a date alone, a year and month, or nothing at all, plus the
first exposure date/time (`TRTSDTM`) from the subject-level analysis dataset
(ADSL), empty when the subject has no ADSL record.

**Variables:**

- `TRTSDTM` is the subject's first exposure date/time.
- `ASTDTM` is the analysis start date/time. Every part of the start that was
  not collected takes its earliest value: the first month, the first day and
  `00:00:00`, or the first second of a collected minute. A start never
  collected takes `TRTSDTM` instead, and stays empty when that is empty too.
- `ASTDTF` names the first part of the date that was supplied: `Y` when the
  whole date came from first exposure, `M` when the month was supplied and
  `D` when only the day was. It is empty when the date was collected in full
  and when there is no analysis start.
- `ASTTMF` names the first part of the time that was supplied: `H` when the
  whole time was, including a start taken from first exposure, `M` when the
  minutes were and `S` when only the seconds were. It is empty when the time
  was collected to the second and when there is no analysis start.

**Note:** a start taken from first exposure counts as entirely supplied,
even though first exposure itself was collected to the second.
