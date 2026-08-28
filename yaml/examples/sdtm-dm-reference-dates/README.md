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
three `RFENDTC` sources win for at least one subject, so every branch is
exercised.

## The bootstrap question

The circular dependency C08 asks about does not arise inside this
specification. `--DY` variables in EX, DS, and AE need `DM.RFSTDTC`, while
`DM.RFXSTDTC` needs EX; because one specification derives one dataset, DM is
simply derived first and the dependent domains read it as an input. R001 cycle
detection is per specification, so nothing here can report a cycle across
datasets either.

That means the language does not need a `_DM_REF` intermediate dataset, but it
also cannot state the ordering it depends on. Which specification runs first is
outside the schema. The multi-output pipeline manifest recorded for C01 and X10
is where that ordering has to be declared.

## Status and named gaps

This fixture is a **probe**. It passes, and it makes three gaps visible.

1. **There is no row-wise maximum.** `min` and `max` reduce one right-side
   dataset; nothing takes the latest of several already-derived columns.
   `coalesce` returns the first non-missing value, not the greatest.
   `RFENDTC` therefore spells the three-way maximum out as `case` branches with
   null-guarded pairwise comparisons. The predicates are correct and
   deterministic, but each additional candidate date adds a branch and widens
   every earlier predicate. A `greatest` expression over a list of variables
   would replace the whole block.
2. **An extreme and its associated values come from two independent
   reductions.** `RFXENDTC` is `max` over `EX.EXENDTC`, while `EXDOSE0` is an
   ordered `source` selection over the same records. Nothing ties them to the
   same EX record: they agree here only because both order by `EXENDTC` and the
   fixture breaks the remaining tie with `EXSEQ`. A single expression returning
   an extreme row, rather than an extreme value, would make the guarantee
   structural. The known limitation that ordered `source.multiple_matches`
   cannot take a filter applies here too, as recorded by
   `../adam-adsl-treatment-selection`.
3. **No match and empty match are indistinguishable.** `CATH-UCSD-0003` has no
   EX record and `CATH-UCSD-0002` has no AE record; both produce missing, and
   so would a subject whose EX records all had missing dates. Diagnostics that
   need to separate "never exposed" from "exposed, dates not collected" have
   nothing to read.

`EXDOSE0`, `DSDT0`, and `AEDT0` are not DM variables. They remain output
columns because named intermediates are unsupported, the same gap recorded by
`../adam-adsl-treatment-selection`.

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
