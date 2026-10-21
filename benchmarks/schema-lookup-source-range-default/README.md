# Each Dose Takes the Period Holding the Start Date the Output Carries

**Goal:** give each dose of study drug its analysis start date (`ASTDT`) and
the number of the subject's treatment period (`APERIOD`) whose days hold that
date.

**Input:** one `spec.yaml` over exposure records, each carrying a sequence
number (`EXSEQ`), the dose given (`EXDOSE`) and a start date (`EXSTDT`), and a
period table giving each subject's treatment periods, each from a first day
(`APERSDT`) to a last day (`APEREDT`).

**Variables:**

- `ASEQ` is the sequence number of the exposure record that gave the dose.
- `ASTDT` is the start date of the dose.
- `APERIOD` is the number of the subject's period whose first and last days,
  both inclusive, hold `ASTDT`. It is blank when none of the subject's
  periods holds the date.

**Note:** the analysis start date is stated once for the whole dataset, not
in the rule that builds the dose rows, and only exposure records that gave a
dose build rows. Each dose row still works out its own start date before its
period is chosen, so the period always follows the date the output carries.
A dose outside every period is kept with the period blank, and each such
blank is counted so the run reports how often it happened.
