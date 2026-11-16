# Derive Baseline BMI with a Sponsor Routine

**Goal:** derive baseline body mass index (`BMIBL`) with the routine the
project supplies, the way a sponsor's standard macro would, and check it
against the formula written inline.

**Input:** subject-level records carrying baseline height in
centimetres (`HEIGHTBL`) and baseline weight in kilograms (`WEIGHTBL`);
both are carried through unchanged.

**Variables:**

- `BMIBL` holds the baseline body mass index from the project routine:
  `WEIGHTBL` divided by the square of height in metres (`HEIGHTBL`
  divided by 100), not rounded. It is empty when `HEIGHTBL` or
  `WEIGHTBL` is missing.

**Note:** before the dataset is written, every value of the routine is
compared with the same index computed by the formula itself, and the run
stops on any difference; the formula's value is not published. A zero
weight gives an index of zero.
