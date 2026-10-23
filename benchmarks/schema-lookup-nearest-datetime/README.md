# Give Each ECG the Concentration Nearest in Time

**Goal:** give each electrocardiogram (ECG) the plasma concentration
measured nearest in time to it, before or after: the sample's sequence
number (`PCSEQ`), date/time (`PCDTM`) and concentration (`PCCONC`).

**Input:** one `spec.yaml` over ECG records, each with its sequence number
(`EGSEQ`) and its date and time, and pharmacokinetic (PK) concentration
records, each with its sequence number, date/time and result.

**Variables:**

- `ADTM`: the ECG date and time.
- `PCSEQ`: the sequence number of the subject's sample nearest in time to
  the ECG. Of two samples equally far from it, the one taken earlier is
  chosen, so a sample before the ECG wins over one as far after it; of two
  taken at the same time, the one recorded first, whatever their sequence
  numbers.
- `PCDTM`: the date and time of that sample.
- `PCCONC`: the concentration of that sample.

**Note:** the distance is the time between the sample and the ECG in either
direction, measured to the second, so a sample 45 minutes before an ECG is
nearer than one 90 minutes after it.
