# Round written ties of large values half away from zero

**Goal:** build `R2`, `R3` and `R4` by rounding analysis values that
are written as exact halfway marks, however many digits precede the
decimal point.

**Input:** pharmacokinetic parameter values, each written as a
halfway mark at two, three or four places, with one value written
just short of a mark.

**Variables:**

- `ID`: the parameter record.
- `AVAL`: the analysis value as collected.
- `R2`: `AVAL` rounded to two decimal places.
- `R3`: `AVAL` rounded to three decimal places.
- `R4`: `AVAL` rounded to four decimal places.

**Note:** a value written as a halfway mark with up to fifteen
significant digits moves away from zero, as a reader of the written
decimal expects, even when its stored binary form falls just below
the mark. A value written short of the mark rounds to the nearer
candidate.
