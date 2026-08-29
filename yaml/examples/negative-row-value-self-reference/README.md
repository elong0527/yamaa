# ADaM ADVS: reject a weight carried forward from a carried-forward weight

This example uses a series of collected weights to attempt one analysis record
per measurement:

- `AVAL` is the collected weight and is empty when no measurement was taken;
- `AVALF` is meant to hold the collected weight, or the most recent earlier
  weight when none was collected.

Two consecutive measurements are missing, so the second gap would have to take
its value from a record that was itself filled in, and `AVALF` is stated in
terms of its own earlier value. Filling one gap and stopping, or resolving the
rule in an order nothing states, would each give a different answer from the
same data, so the run must fail and no artifact is accepted.
