# SDTM DM reference dates and participation bounds

This focused probe answers one question: how are the DM reference dates derived
from the domains that DM itself is the reference for?

## Rule and record grain

`DM_RAW` is the base, so each enrolled subject produces one DM row. `RFXSTDTC`
and `RFXENDTC` reduce EX by subject. `RFSTDTC` is this study's reference start,
defined as the first exposure. `RFENDTC` is the last participation date, the
latest of the last exposure end, the last disposition-event date, and the last
adverse-event end date.

The four subjects cover consent before treatment, two qualifying exposures,
early discontinuation, an adverse event ending after the last dose, a screen
failure with no exposure at all, and a subject with no disposition record. All
three `RFENDTC` sources win for at least one subject, and two subjects are
missing one of them, so `greatest` is exercised over a full set, a partial set,
and a set whose first candidate is absent.

## The bootstrap question

The apparent circular dependency does not arise inside this specification.
`--DY` variables in EX, DS, and AE need `DM.RFSTDTC`, while `DM.RFXSTDTC` needs
EX; because one specification derives one dataset, DM is derived first and the
dependent domains read it as an input.

So the language needs no intermediate dataset, but it also cannot state the
ordering it depends on. Which specification runs first is outside the schema,
and R001 cycle detection is per specification, so a cycle across datasets
cannot be reported either. Declaring that ordering needs the multi-output
pipeline manifest.

## Gaps this fixture names

1. **Closed by `greatest`.** `RFENDTC` is a row-wise maximum over three dates,
   and it is now written as one expression over named columns. It previously
   spelled the three-way maximum out as `case` branches with null-guarded
   pairwise comparisons, where each additional candidate date added a branch
   and widened every earlier predicate. R010's `GREATEST` stays numeric, `min`
   and `max` reduce one right-side dataset, and `coalesce` returns the first
   non-missing value rather than the greatest, so none of the three covered
   this; the registry entry does, for any comparable type. A fourth candidate
   date is now one more entry in `sources`.
2. **An extreme and its associated values come from two independent
   reductions.** `RFXENDTC` is `max` over `EX.EXENDTC`, while `EXDOSE0` is an
   ordered `source` selection over the same records. Nothing ties them to the
   same EX record: they agree here only because both order by `EXENDTC` and the
   fixture breaks the remaining tie with `EXSEQ`. Both can declare the same
   `filter`, so the two reductions can be made to see the same records, but
   nothing ties them to the same one. A single expression returning an extreme
   row, rather than an extreme value, would make the guarantee structural.
3. **No match and empty match are indistinguishable.** `CATH-UCSD-0003` has no
   EX record and `CATH-UCSD-0002` has no AE record; both produce missing, and
   so would a subject whose EX records all had missing dates. Diagnostics that
   need to separate "never exposed" from "exposed, dates not collected" have
   nothing to read.

`EXDOSE0`, `DSDT0`, and `AEDT0` are not DM variables and declare
`output: false`, so the artifact is conformant DM while the three candidate
dates that feed `RFENDTC` stay internal. The `exposure-reference-completeness`
verification still names `EXDOSE0`.

`RFSTDTC` duplicates `RFXSTDTC` because this study defines the reference start
as the first exposure. They are distinct SDTM variables and the equality is a
study decision, not a rule.

## Diagnostics and verifications

Expected `EXDOSE0.source.multiple_matches` count is one, for the subject with
two exposure records. No handler path is declared.

Rows remain in `DM_RAW` order; the key is `[STUDYID, USUBJID]`; exactly four
rows are expected. The exposure reference variables must be present or absent
together, the exposure window must be ordered, consent must not follow first
exposure, and participation must not end before the last exposure.
