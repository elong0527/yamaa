# Window Home Measurements by Their Own Study Day

**Goal:** give each clinic and home measurement its analysis window
(`WINDOW`), next to where it was taken (`SOURCE`), its sequence number
(`SEQ`), and its study day (`DAY`).

**Input:** one `spec.yaml` over clinic measurements, which carry the visit
they were collected at; home measurements, which carry no visit and store
their fields in a different order; and a window table giving each analysis
window of the study its first and last study day.

**Variables:**

- `SOURCE` is `CLINIC` for a clinic measurement and `HOME` for a home one.
- `WINDOW` is the collected visit for a clinic measurement. For a home
  measurement it is the window whose days, both ends inclusive, hold the
  study day of that home record; blank when no window does.

**Note:** a home row's window is chosen by the day of the home record that
built the row, never by a clinic value. Window edges are inclusive, so day
21 closes `WEEK 2` and day 22 opens `WEEK 4`.
