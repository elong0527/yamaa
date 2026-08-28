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

## Conversions without division or rounding

Neither conversion can be written the way it is usually stated.

- Pounds to kilograms is a single `multiply` by `0.45359237`, which is exact
  in the language.
- Fahrenheit to Celsius is `(F - 32) * 5/9`. `subtract` takes two variables, so
  it cannot subtract the literal `32`; the fixture uses `add` with an `addend`
  of `-32` instead. There is no `divide`, so `5/9` is written as the decimal
  literal `0.5555555555555556`.

`multiply` also takes a literal `factor`, never a variable, so a per-row
conversion factor is impossible. Each unit pair needs its own `case` branch
holding its own nested `multiply`, and the branch list grows with the number of
collected units rather than with the number of tests.

There is no `round`. `175 LB` standardizes to `79.37866475 kg`, which is the
exact product and not a submission-ready weight. The fixture commits that value
rather than a rounded one because rounding is not expressible today.

## Status and named gaps

This fixture is a **probe**. It makes four gaps visible.

1. **No `divide` and no `round`.** Both appear in almost every unit conversion.
   Their absence forces reciprocal literals and unrounded results.
2. **`subtract` cannot take a literal.** Its `minuend` and `subtrahend` are both
   typed `variable`, while `add` accepts a literal `addend`. The two numeric
   expressions are asymmetric, and only the `add` form is usable here.
3. **Arithmetic has no declared missing policy.** R007 defines `missing`
   handlers for `mapping`, `mapping_from`, `cut`, and the string expressions,
   but says nothing about `add`, `subtract`, or `multiply` receiving a missing
   input. Every branch in this fixture therefore guards with an explicit
   `IS NOT NULL` predicate so the last record's behavior is defined by the
   specification rather than by the runtime. A rule should state whether
   arithmetic propagates missing or fails.
4. **Float-to-string conversion is undefined.** `VSSTRESC` is the character
   form of `VSSTRESN` and is derived by declaring `type: str` over the same
   value, which relies on the R005 conversion matrix that is still unresolved.

`VSORRESN` and `TEMPADJ` are not SDTM variables. They remain output columns
because named intermediates are unsupported, the same gap recorded by
`../adam-adsl-treatment-selection`.

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
