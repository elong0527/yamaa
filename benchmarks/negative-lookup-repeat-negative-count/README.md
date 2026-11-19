# Reject a Dose Record That Ends Before It Starts

**Goal:** give one row for each day a collected exposure (EX) record
covers, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE` and
`EXDOSU` through, numbering each administration in `ADOSEN` and dating it
in `ASTDT`.

**Input:** one `spec.yaml` over collected exposure records of once-daily
dosing, each with the first and last day it covers (`EXSTDTC`, `EXENDTC`),
one of them with an end date before its start date.

**Variables:**

- `ADOSEN` numbers the administrations built from one collected record, in
  date order from `1`.
- `ASTDT` is the day of the administration: the collected record's start
  date for the first, and one day later for each one after it.

**Note:** a collected record that ends before it starts covers no day that
can be counted, so it rejects the run, and no artifact is accepted.

## How to fix

Correct the exposure dates: an end date before the start date is a data
error, and the record that carries it is named in the failure. Do not turn
a negative count into zero to make the run pass. The record would then
build no administration and leave the dataset without a word, which is
the loss the count exists to prevent.
