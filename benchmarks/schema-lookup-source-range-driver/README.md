# Each Dose Takes the Treatment Period Holding Its Own Start Date

**Goal:** give each dose of study drug the number of the subject's treatment
period (`APERIOD`) whose days hold the start date of that dose.

**Input:** one `spec.yaml` over exposure records, each carrying a sequence
number (`EXSEQ`), the dose given (`EXDOSE`) and a start date (`EXSTDT`), and a
period table giving each subject's treatment periods, each from a first day
(`APERSDT`) to a last day (`APEREDT`).

**Variables:**

- `ASEQ` is the sequence number of the exposure record that gave the dose.
- `APERIOD` is the number of the subject's period whose first and last days,
  both inclusive, hold the start date of the dose. It is blank when none of
  the subject's periods holds the date.

**Note:** only exposure records that gave a dose build rows. Each dose is
placed by its own start date, so a subject with several doses gives each one
the period that holds it, and the output carries no copy of the date. A dose
outside every period is kept with the period blank, and each such blank is
counted so the run reports how often it happened.
