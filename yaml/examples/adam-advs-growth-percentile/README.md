# ADaM ADVS: express a measurement as a growth percentile

This example uses collected body measurements, a sex-and-age growth reference,
and a `yamaa` specification to derive one row per collected measurement:

- `PARAMCD` and `PARAM` restate the measurement as the percentile it converts
  to: a body mass index becomes a BMI-for-age percentile and a weight a
  weight-for-age percentile;
- `AVAL` is that percentile. The measurement, the subject's sex, and the
  subject's age in days select the reference coefficients that convert it; a
  measurement with no matching reference leaves the percentile empty.

A percentile says where a measurement stands among children of the same sex
and age, so the same value means different things at different ages and is not
compared to the collected number. The growth reference is study data, not a
fixed constant, and is supplied beside the measurements.
