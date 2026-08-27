# ADaM ADSL treatment and disposition

This standalone SDTM-to-ADaM probe derives one ADSL row per DM subject using
EX treatment intervals and DS disposition records. It is a compact version of
the CATH ADSL pattern with values chosen to expose selection and missing-data
behavior.

## Status and boundary

This fixture is a **probe**. Every expression is registered, but several
behaviors depend on draft R004 and R005 semantics. It covers only SDTM to ADaM;
there is no upstream ODM stage in this fixture.

The current language can filter `min` and `max` date reductions, but an ordered
`source.multiple_matches` selection cannot use the same filter. Therefore all
EX records in this positive fixture are treatment administrations, and every
subject's chronologically final DS record is a disposition record. A future
negative companion should place a non-treatment EX record first or a later
non-disposition DS record last and require filtered associated-row selection.

## Business rule and record grain

DM is the base and produces exactly one output row per subject. The derivation:

- parses the site from a standard `USUBJID`, falling back to DM `SITEID` when
  the identifier is malformed;
- constructs a stable subject reference from site and subject identifiers;
- normalizes country and derives a region;
- selects the first treatment name and the earliest/latest qualifying exposure
  dates independently of source-file order;
- treats placebo EX records with `EXDOSE = 0` as real administrations;
- calculates inclusive treatment duration as date difference plus one;
- selects the final DS decode and reason by date and sequence;
- derives end-of-study, safety, and intent-to-treat flags.

## Challenge cases

The four subjects distinguish:

- active treatment split across two EX intervals stored out of chronological
  order;
- placebo treatment split across two zero-dose intervals, which must still set
  treatment dates and `SAFFL = Y`;
- a randomized subject with no EX and no DS, producing planned treatment,
  missing actual-treatment dates, `SAFFL = N`, and `EOSSTT = ONGOING`;
- a screen-failure subject with missing `SUBJID`, arm, treatment, and country;
- a malformed `USUBJID` whose parsed site is missing and whose collected
  `SITEID` supplies the fallback;
- two same-day discontinuation records where `DSSEQ` selects the corrected
  final reason;
- protocol-milestone DS rows that are excluded from `EOSDT` reduction.

`TRT01RAW`, `TRT01SRC`, `TRTDUR0`, `EOSDECOD`, and `EOSREAS` expose selection
and dependency steps. Some are useful traceability variables; others would
normally be named non-output intermediates. Their presence demonstrates the
current internal-intermediate design gap rather than hiding it in a function.

## Schema and rule coverage

The fixture is shaped for `../../schema.yaml`. It exercises dependency
ordering from R001; source binding from draft R002; many-to-one joins, filtered
right-side reduction, no-match behavior, and ordered multiple-match handling
from R003/R008; predicates, string operations, date arithmetic, conditional
logic, and aggregates from draft R004 and R007; and output conversion, keys,
and verifications from draft R005/R009.

## Deterministic output

Rows remain in DM source order. The exact key is `[STUDYID, USUBJID]`, and four
rows are expected.

`TRTSDT` is the minimum qualifying `EXSTDTC`; `TRTEDT` is the maximum qualifying
`EXENDTC`. `TRT01RAW` sorts EX matches by `EXSTDTC`, then `EXSEQ`, and keeps the
first. `EOSDECOD` and `EOSREAS` sort DS matches by `DSSTDTC`, then `DSSEQ`, and
keep the last. These selections do not depend on CSV record order.

## Diagnostics and verifications

Expected handler counts are:

- `SITEIDP.str_extract.missing`: 0;
- `SITEIDP.str_extract.no_match`: 1;
- `SUBJREF.str_concat.missing`: 1;
- `COUNTRYUC.str_upper.missing`: 1;
- `REGION1.mapping.unmapped`: 1;
- `TRT01RAW.source.multiple_matches`: 2;
- `EOSDECOD.source.multiple_matches`: 2;
- `EOSREAS.source.multiple_matches`: 2.

All column verifications pass. Dataset verifications require four unique
subject keys; treatment dates and duration to be all missing or all present;
placebo subjects to be included in the safety population; and discontinued
subjects to have a non-missing reason. Each business rule has a stable ID for
failure reporting.
