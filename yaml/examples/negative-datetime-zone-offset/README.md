# ADaM ADAE: reject an event start recorded against another clock

This example uses collected adverse events to attempt one record per event:

- `ASTDTM` is meant to be the moment each event started.

One start is a plain reading of a wall clock and the other carries an offset
from one. The two are not the same kind of value, and holding both in one
column would first need a rule saying which clock a result is read on. Shifting
the offset value to some other clock would move a collected time, and keeping
the offset beside it would leave two records that cannot be ordered against
each other, so the run must fail and no artifact is accepted. A study that
records an offset keeps it in a column of its own, where it stays readable.
