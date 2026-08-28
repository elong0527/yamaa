# ADaM ADAE partial date imputation

This focused SDTM-to-ADaM probe answers one question: can a partial collected
date be turned into an analysis date and an imputation flag without a project
function?

## Rule and record grain

AE is the base, so each event produces one ADAE row. The imputation rule is the
common one: a missing day becomes `01`, a missing month becomes `01`, and the
flag records the coarsest imputed component. `ASTDTF` is `M` when the month was
imputed, `D` when only the day was imputed, and missing when the collected date
was complete. A collected value with no parseable year yields no analysis date
at all.

The six events cover a complete date, a year-month date, a year-only date,
non-date text, an uncollected date, and a second complete date after treatment
start.

## The answer: string surgery, not date handling

The derivation works, and it works entirely on strings. `str_extract` pulls the
year, month, and day with anchored patterns; `coalesce` supplies the imputed
default for each absent component; `str_concat` rebuilds an ISO date; and only
then does a declared `type: date` convert the rebuilt text.

Nothing in the language knows that `2025-01` is a date of month precision. The
components are recovered by regular expression, the precision is inferred from
which extraction found no match, and the imputation defaults are string
literals `'01'` rather than date parts. Branch order in `ASTDTF` carries real
meaning: the `YR0 IS NULL` guard must come first, because otherwise an
unparseable value would report an imputed month.

This fixture has more intermediate columns than analysis columns. `YR0`, `MO0`,
`DA0`, `MOI`, and `DAI` all have to be emitted because named intermediates are
unsupported, the same gap recorded by `../adam-adsl-treatment-selection`.

## Imputation silently decides treatment emergence

`TRTEMFL` is `Y` when `ASTDT` is on or after `TRTSDT`, which is `2025-01-10`
here. `../adam-adae-treatment-emergent` covers the full inclusive interval; this
fixture keeps only the lower bound so the effect of imputation is visible.

Event two was collected as `2025-01`. Imputing day `01` places it before
treatment start and it is not flagged, yet twenty-one of the thirty-one
possible days would have made it treatment-emergent. Event three, collected as
`2025`, is decided the same way from even less information. The output records
no measure of that uncertainty: `ASTDTF` says a component was imputed, but
nothing downstream distinguishes a confident `TRTEMFL` from a coerced one.

## Status and named gaps

This fixture is a **probe**. It passes, and it names four gaps.

1. **There is no date type with precision.** A declared `date` is complete or
   it is nothing. Partial dates must live as strings until they are made whole,
   so every specification that touches them repeats this extraction block.
2. **Imputation is not a language concept.** The rule, the defaults, and the
   flag are all hand-written. Two studies writing the same standard rule will
   write it differently, and neither R nor Python is told what the rule was.
3. **Comparisons under uncertainty are unmarked.** An imputed date compares
   exactly like a collected one. A rule stating how an interval comparison
   behaves when an operand is imputed, or a way to propagate that uncertainty,
   does not exist.
4. **Only trailing precision loss is covered.** SDTM also represents a known
   day in an unknown month. That form needs an agreed representation before a
   fixture can assert it, so this fixture deliberately stops at ISO 8601
   truncation and records the rest as an open question. Conflicting start and
   end precision, also listed for X04, needs an end date and is not covered
   here.

## Diagnostics and verifications

Expected handler counts are exact. `YR0` has one `missing` and one `no_match`;
`MO0` has one `missing` and two `no_match`; `DA0` has one `missing` and three
`no_match`. Every declared handler is used at least once.

`ASTDT` converts rebuilt text to a date and every rebuilt value is well formed,
so no `conversion_failure` handler is declared; the unparseable and uncollected
rows never reach conversion because `ASTDTC` is already missing.

Rows remain in AE order; the key is `[STUDYID, USUBJID, AESEQ]`; exactly six
rows are expected. The analysis date and its character form must be present or
absent together, a flag requires a date, and a completely collected date must
never be flagged.
