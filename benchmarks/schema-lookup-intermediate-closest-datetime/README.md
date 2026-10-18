# Select the Concentration Closest in Time to Each ECG

**Goal:** give each electrocardiogram (ECG) the plasma concentration
measured closest in time to it, before or after: the sample's sequence
number (`PCSEQ`), date/time (`PCDTM`) and concentration (`PCCONC`), and the
hours between the sample and the ECG (`PCDIST`).

**Input:** one `spec.yaml` over ECG records, each with its sequence number
(`EGSEQ`) and its date and time, and pharmacokinetic (PK) concentration
records, each with its sequence number, collected date/time text and
result.

**Variables:**

- `ADTM`: the ECG date and time; empty when none was recorded.
- `PCSEQ`: the sequence number of the subject's sample closest in time to
  the ECG; empty when no eligible sample exists. Of two samples equally
  close, the one taken before the ECG is chosen, whatever their sequence
  numbers; of two taken at the same time, the one with the lower sequence
  number.
- `PCDTM`: the date and time of that selected sample; empty when no eligible
  sample exists.
- `PCCONC`: the concentration of that selected sample; empty when no
  eligible sample exists.
- `PCDIST`: hours between `PCDTM` and `ADTM`, without direction and not
  rounded.

**Note:** an hour distance needs a clock time on both sides. A sample whose
time was not recorded to the minute, such as one recorded with a date
alone, is never chosen, instead of being placed at midnight, and an ECG
with no date and time is paired with no sample. Its four sample columns are
then empty, as they are when no sample of the subject has such a time.
