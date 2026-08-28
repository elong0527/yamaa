# SDTM VS unit standardization

This focused collected-to-SDTM probe answers one question: how does one
collected result produce both an original record and a standardized result?

## Rule and record grain

`VS_RAW` is the base, so each collected vital-signs record produces one VS row.
`VSORRES` and `VSORRESU` keep the value and unit exactly as collected.
`VSSTRESN`, `VSSTRESC`, and `VSSTRESU` carry the standardized result in the
study's standard unit for the test: `cm` for height, `kg` for weight, and `C`
for temperature.

The six records cover a metric site that needs no conversion, a US site
collecting pounds and Fahrenheit, and a temperature that was not collected.
Original and standardized values are separate columns throughout; nothing
overwrites the collected result.

## Conversions as written

Each conversion is one `compute` formula, stated the way a protocol states it:

- Pounds to kilograms is `VSORRESN * 0.45359237`.
- Fahrenheit to Celsius is `(VSORRESN - 32) * 5 / 9`.

`NULL` propagates, so the uncollected temperature in the last record
standardizes to missing with no guard in the specification.

**The two spellings of the temperature conversion are not interchangeable.**
`(VSORRESN - 32) * 5 / 9` returns exactly `37` for `98.6 F`, and
`(VSORRESN - 32) / 1.8` returns `36.99999999999999`. Both are correct IEEE 754;
they differ because they associate differently. This is why R010 requires an
implementation to evaluate a formula as written and forbids reassociating it.

There is no rounding. `175 LB` standardizes to `79.37866475 kg`, the exact
product. R010 has no rounding function: a derivation carries full precision and
rounding is a reporting concern.

## Two gaps this fixture names

**A conversion factor cannot be data.** `compute` accepts a column in any
operand position, but the factor for a unit pair lives in a dictionary, and
there is no way to bring a looked-up value into an expression as a term. Each
unit pair needs its own `case` branch, and the branch list grows with the
number of collected units.

**Float-to-string conversion is undefined.** `VSSTRESC` is the character form
of `VSSTRESN`, derived by declaring `type: str` over the same value, and R011
defines every conversion in its matrix except this one.

The expected output commits `37`, not `37.0`, for the standardized Fahrenheit
record. This proposes that float-to-string conversion render the shortest form
that round-trips to the same value and drop a trailing zero for an integral
value. R and Python disagree by default here, so an implementation producing
`37.0` today is not wrong; it is evidence that the rule must be stated. The
same rule governs whether `79.37866475` may ever be shortened, which it must
not be.

## Diagnostics and verifications

No handler path is declared. `VSORRESN` converts a collected character result
to float, and every value in this fixture converts cleanly, so no
`conversion_failure` handler is present; `../sdtm-lb-multiform` covers the
failing path. `VSORRESN` is not an SDTM variable and declares `output: false`,
so the `unconverted-result-is-unchanged` verification names an internal column,
which R005 allows: the assertion is about the conversion, not the artifact.

Rows remain in `VS_RAW` order; the key is `[STUDYID, USUBJID, VSSEQ]`; exactly
six rows are expected. The standardized value, its character form, and its unit
must be present or absent together, and a record already collected in the
standard unit must standardize to the identical number.
