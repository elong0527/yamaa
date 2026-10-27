# Subject-Level Analysis Data Submitted in SAS Transport with ADaMIG 1.1

**Goal:** carry `TRT01A`, `AGE`, `HEIGHTBL`, `TRTSDT` and `TRTSDTM` into
ADSL and submit it as the SAS transport file `adsl.xpt`, with the dates as
SAS numeric dates.

**Input:** one pre-analysis row per subject carrying the actual treatment,
age, baseline height and the collected date and time of first exposure.

**Variables:**

- `TRT01A` is the actual treatment.
- `AGE` is the age in years, shown in three positions.
- `HEIGHTBL` is the baseline height in centimetres, shown with one decimal;
  missing when not measured.
- `TRTSDT` is the date part of `TRTSDTM`; missing when the subject was never
  treated.
- `TRTSDTM` is the date and time of first exposure; missing when the subject
  was never treated.

**Note:** in the transport file `TRTSDT` counts days and `TRTSDTM` seconds
from 1 January 1960, shown as `DATE9.` and `DATETIME20.`, and the
data-definition document describes both as eight-byte integers with those
formats. Each text variable is exactly as wide as its longest value.

The `project.yaml` guide declaration is ADaMIG 1.1; the data-definition
document records it and points to the SAS Transport version 5 dataset.
