# SDTM VS: standardize collected results into the study's units

This example uses collected vital signs and a `yamaa` specification to derive
one row per collected record, describing each vital sign on its own:

- `VSORRES` and `VSORRESU` are the result and the unit exactly as collected,
  and nothing overwrites them;
- `VSSTRESN` is that result expressed in the study's standard unit for the
  test, and `VSSTRESU` is that unit. Height is already in centimetres. Weight
  is kilograms, so pounds are multiplied by 0.45359237. Temperature is Celsius,
  so Fahrenheit becomes (F - 32) * 5 / 9. A result already collected in the
  standard unit passes through unchanged;
- `VSSTRESC` is the same standardized value written as text, so it and
  `VSSTRESN` always agree.

Adding a vital sign means describing that one test rather than extending a rule
shared by all of them, and records leave grouped by test in that order rather
than in collection order.

A result that was not collected has no standardized value, no character form,
and no unit. Nothing is rounded: a standardized value keeps the full precision
of the arithmetic, and the four decimal places in the output file are how it is
written rather than what is stored.
