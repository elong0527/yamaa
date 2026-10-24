# Group subjects by the study's baseline BMI quartiles

**Goal:** derive the study's first quartile (`BMIBLQ1`), median
(`BMIBLMED`) and third quartile (`BMIBLQ3`) of baseline body mass index
(`BMIBL`), and place each subject in the quartile group (`BMIBLGR1`) that
its baseline value falls in.

**Input:** one subject-level record per subject with the baseline body mass
index in kg/m2, carried through unchanged and blank for a subject without a
baseline measurement.

**Variables:**

- `BMIBLQ1`, `BMIBLMED` and `BMIBLQ3` hold the quartiles of the baseline
  values of the subjects in the study that have one, the same on every
  record, including that of a subject without a value. Each follows R's
  default definition (type 7): with the values sorted, a quartile whose
  position falls between two of them lies between them in proportion to
  that position, so a cutpoint need not be an observed value. They are not
  rounded.
- `BMIBLGR1` is `Quartile 1` for a value at or below the first quartile,
  `Quartile 2` up to the median, `Quartile 3` up to the third quartile and
  `Quartile 4` above it, so a value on a cutpoint falls in the lower group.
  It is blank for a subject without a baseline value.

**Note:** SAS's default percentile definition returns an observed value or
the midpoint of two, so its first and third quartiles of these values differ
from R's; the median is the same under both definitions.
