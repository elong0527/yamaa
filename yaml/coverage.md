# Coverage against real derivation requirements

What this design must express, measured against two working implementations in
this repository: the `cath/` study specifications and ADaM scripts, and the
`cdiscbuildeR/` engine. This is the working list for closing the gap, and it is
the first thing to read before extending the design.

Status values: **yes** expressible and exercised by a fixture; **untested**
expressible but no fixture; **no** not expressible.

## ODM to SDTM

| Requirement | Status | Notes |
|---|---|---|
| Direct item source, ItemOID to variable | yes | `sdtm-lb-findings` |
| Structural key rename, `SubjectKey` to `USUBJID` | yes | every fixture |
| **Deriving** `USUBJID` by concatenation | **no** | no string-construction operation; `STUDYID-SITEID-SUBJID` is inexpressible except through `call`. Every fixture dodges this by copying a pre-built value |
| Literal constant | yes | every fixture |
| Codelist recode, with case sensitivity | yes | `adam-adsl-mapping` |
| Type cast | yes | R005 conversion |
| Copy from an already-derived output column | yes | `sdtm-dm-basic` `ACTARM` |
| Cross-domain lookup on inferred keys | yes | `adam-adlb-bds` |
| Cross-domain lookup on **explicit** join keys | **no** | `cdiscbuildeR` `merge_on`; R003 infers keys from output `keys` only, so a join on `ItemGroupRepeatKey` cannot be stated |
| Sequence numbering | yes | `row_number` |
| Named function call | yes | `call`, with a `study_day` probe; the host library is `tests/python/functions.py` |
| Wide-to-tall explosion by enumeration | yes | `rows` templates |
| Multi-source union into one domain | yes | `sdtm-relrec-related-records` |
| Row filtering of source records | yes | `row.filter` |
| Dictionary lookup from an external file | yes | `sdtm-ae-dictionary-coding` |
| Per-group extreme record selection | partial | `baseline_flag` only; no general first/last with ordering |
| Templating for repeated near-identical blocks | **no** | `cath/sdtm/specs/lb.yaml` is 19 near-identical blocks, two lines apart |
| `--BLFL` baseline flags | **no** | needs per-group extreme flag before first dose |
| `VISIT`/`VISITNUM` from a visit-schedule lookup | untested | `mapping_from` should cover it |

## SDTM to ADaM

| Requirement | Status | Notes |
|---|---|---|
| ADSL variable propagation | yes | R003 join |
| Baseline flag, value, change, percent change | yes | `adam-adlb-bds` |
| Analysis sequence numbering | yes | `row_number` |
| Conditional recode, `case_when` | untested | `case` |
| Derive only where a condition holds | yes | `derivation.where`, `adam-adlb-bds` `PCHG` |
| Range banding | untested | `cut` |
| First non-missing | untested | `coalesce` |
| Study day, no day 0 | yes | `call` with `study_day` |
| Duration | untested | `date_diff` plus `add` |
| First and last dose date | **no** | needs aggregate with ordering and a filter on the added dataset |
| Existence flag from another dataset | **no** | `SAFFL` from "any EX record with dose > 0" |
| Population and treatment-emergent flags | untested | `case` over output columns |
| Occurrence flags, `AOCCFL` and friends | partial | `where` restricts the population; still needs first/last with ordering |
| Ordered-categorical to numeric rank | untested | `mapping` |

## The remaining gaps

Frequency in `cath/` is how these were first ordered, and that was the wrong
criterion: `cath/` is one short single-site study that hardcodes `EPOCH`,
hardcodes every unit, and has no ADTTE, no multi-period design, and no partial
dates. It bounds what these gaps look like, not what the requirement space is.

One of the original seven is closed. Gaps 8 to 12 were missed on the first pass
and found by review; several outrank the ones already listed.

1. ~~**Restricted derivations.**~~ **Closed.** `derivation.where` restricts a
   derivation to the output rows satisfying a predicate, leaving the rest
   missing, and a window operation inside it partitions only those rows.
   Exercised by `PCHG` in `adam-adlb-bds`, which belongs only on post-baseline
   records. This is the declarative form of `restrict_derivation`.

2. **Extreme-record selection with ordering and a filter.** First dose is
   "earliest `EXSTDTM` among EX records with `EXDOSE > 0`, ordered by
   `EXSTDTM, EXSEQ`". R003 reduction has `derivation.filter` and a leading
   aggregate, but no ordering and no `first`/`last` verb.

3. **Explicit join keys and cardinality.** R003 infers applicable keys from
   output `keys`. `cdiscbuildeR` declares `merge_on` and silently dedups the
   right side. Neither states cardinality, ordering, or a match-mode.

4. **Multi-output derivations.** `derive_vars_dtm` emits both `EXENDTM` and
   `EXENTMF` from one call. The model is one derivation per column, so
   prefix-based fan-out cannot be expressed.

5. **Derived records.** `DTYPE` rows for LOCF, averages and worst-case, and
   computed parameters such as BMI. Row templates construct from a source
   dataset; nothing constructs rows from already-derived output rows.

6. **Intermediate datasets.** `param_lookup` is built inline by
   distinct-arrange-rank and then joined. Nothing declares an ephemeral derived
   dataset.

7. **Templating.** Nineteen LB blocks differing in two lines each. No loop,
   anchor, or parameterized template.

8. **String construction.** There is no `concat`, `upper`, `lower`, `trim`,
   `substr`, or `round`. `str_extract` is the only string operation, although
   R004 points at the stringr vocabulary. This blocks `USUBJID`, `--SPID`,
   composite match keys, and `--STRESN` rounding to study-specified digits.

9. **Referencing a derived dataset.** `datasets` maps identifiers to source
   files only, so a specification cannot read the output of another
   specification. This is the same wound as SUPPQUAL: `SUPPEX` needs
   `IDVARVAL` pointing at the parent's derived `EXSEQ`, which exists nowhere on
   disk. `cath/` relies on it constantly through `_DM_REF`. `schema.yaml`
   declares `parents` for what looks like exactly this and no rule, example, or
   validator uses it. `adam-adlb-bds` only works because ADSL is supplied as a
   pre-built CSV, so the build order is punted outside the design.

10. **Non-equality joins.** R003 matches on equality only. `EPOCH` from
    `SESTDTC`/`SEENDTC`, multi-period `TRTxxP` from `APxxSDT`/`APxxEDT`,
    `ONTRTFL`, a `TRTEMFL` with an end window, and all of ADTTE need interval
    comparison and are structurally impossible.

11. **Join-miss defaults.** R003 returns missing on no match and no exception
    covers it; `missing_source` is stage `bind` and scoped to a variable that
    does not exist. `EOSSTT = "ONGOING"` and `SAFFL = "N"` on no match cannot be
    stated.

12. **Intermediate columns.** Every value must be a published output column,
    because there is no temporary column and arguments cannot nest operations.
    For SDTM and ADaM, where the variable list is fixed by the define, that
    forces throwaway variables into the deliverable. This is the cost of
    forbidding rebinding without adding either escape.

## Deliberately out of scope

Column-set complement selection, label attachment by name-suffix regex, and XPT
export mechanics are packaging concerns rather than derivation semantics.

## What the two engines do that this design rejects

Both reference engines resolve columns in file order and let a later column read
an earlier one, so correctness depends on YAML mapping order. `cdiscbuildeR`
also lets a derivation overwrite an already-derived variable. R001 infers
dependencies instead and forbids rebinding, which is the stricter and more
analyzable choice. It should stay that way, and this list should be closed
without giving it up.
