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

Each conversion is now one `compute` formula, stated the way a protocol states
it:

- Pounds to kilograms is `VSORRESN * 0.45359237`.
- Fahrenheit to Celsius is `(VSORRESN - 32) * 5 / 9`.

Before R010 neither could be written that way. `subtract` typed both operands
as variables and so could not subtract the literal `32`, which forced an `add`
with an `addend` of `-32` into a separate `output: false` column; and with no
`divide`, `5/9` had to be the decimal literal `0.5555555555555556`.

**The two spellings are not interchangeable.** `(VSORRESN - 32) * 5 / 9`
returns exactly `37` for `98.6 F`, and `(VSORRESN - 32) / 1.8` returns
`36.99999999999999`. Both are correct IEEE 754; they differ because they
associate differently. This is why R010 requires an implementation to evaluate
a formula as written and forbids reassociating it.

A per-row conversion factor is still impossible for a different reason: the
factor would have to come from a per-unit dictionary, and `mapping_from` is a
column lookup rather than a term in an expression. Each unit pair still needs
its own `case` branch, and the branch list still grows with the number of
collected units.

There is no rounding. `175 LB` standardizes to `79.37866475 kg`, the exact
product. R010 requires this: a derivation carries full precision and rounding
is a reporting concern.

## Status and named gaps

This fixture is a **probe**. It makes four gaps visible.

1. **Closed: no `divide`, no literal subtrahend.** Both were named here first
   and both are answered by `compute` under R010.
2. **Closed: arithmetic had no declared missing policy.** Every branch used to
   guard with an explicit `IS NOT NULL` predicate, because R007 said nothing
   about `add`, `subtract`, or `multiply` receiving a missing input. R010 fixes
   it: `NULL` propagates, so the uncollected temperature in the last record
   standardizes to missing with no guard in the specification.
3. **A conversion factor still cannot be data.** The remaining half of the
   original gap: `compute` accepts a column in any operand position, but the
   factor for a unit pair lives in a dictionary, and there is no way to bring a
   looked-up value into an expression as a term.
4. **Float-to-string conversion is undefined.** `VSSTRESC` is the character
   form of `VSSTRESN` and is derived by declaring `type: str` over the same
   value, which relies on the R005 conversion matrix that is still unresolved.

`VSORRESN` is not an SDTM variable and declares `output: false`. The former
`TEMPADJ` column is gone: it existed only to hold `F - 32` between two
operations.
The `unconverted-result-is-unchanged` verification still compares `VSSTRESN`
with the internal `VSORRESN`, which is exactly the case R005 allows: the
assertion is about the conversion, not about the artifact.

## Proposed rule for `VSSTRESC`

The expected output commits `37`, not `37.0`, for the standardized Fahrenheit
record. This proposes that float-to-string conversion render the shortest form
that round-trips to the same value and drop a trailing zero for an integral
value. R and Python disagree by default here, so an implementation producing
`37.0` today is not wrong; it is evidence that R005 must state the rule. The
same rule governs whether `79.37866475` may ever be shortened, which it must
not be.

## Diagnostics and verifications

No handler path is declared. `VSORRESN` converts a collected character result
to float, and every value in this fixture converts cleanly, so no
`conversion_failure` handler is present; `../sdtm-lb-multiform` covers the
failing path.

Rows remain in `VS_RAW` order; the key is `[STUDYID, USUBJID, VSSEQ]`; exactly
six rows are expected. The standardized value, its character form, and its unit
must be present or absent together, and a record already collected in the
standard unit must standardize to the identical number.
