# A Column Default Reads Its Own Driver Beside Another Driver's Groups

**Goal:** show that a column's stated default may read a field of a row
template's driver when no template groups that driver, even though another
template groups a different input: clinic measurements take their visit
from the clinic record, while home readings are averaged per subject and
study day (`VSDY`).

**Input:** one `spec.yaml` with two row templates. `clinic` keeps each
clinic measurement (`input/clinic.csv`: one per visit, with the visit name,
study day and systolic blood pressure). `home` builds one record per
subject and study day from the home readings (`input/home.csv`: study day
and systolic blood pressure, several possible on one day).

**Variables:**

- `VSCAT` is `CLINIC` for a clinic measurement and `HOME` for a day of home
  readings.
- `VSSTRESN` is the systolic blood pressure: the measurement itself on a
  clinic record, and the mean of the day's readings on a home record.
- `VSDRVFL` is `Y` on a home record, whose result is computed from the
  day's readings, and blank on a clinic record.
- `VISIT` is the visit of the clinic measurement, stated once by the column
  for every template. The home template states its own blank value, so only
  clinic records read it, each from its own measurement.

**Note:** grouping belongs to the driver a template groups. The home
template's groups limit what a home record reads from the home readings and
say nothing about a clinic field, so the column reading the clinic record
would be refused only by a template that groups the clinic measurements
without that field.
