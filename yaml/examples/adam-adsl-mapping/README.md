# ADaM ADSL value mapping

This fixture covers one thing: translating the values of a source variable into
a standardized vocabulary. It is one row per subject with no row templates, so
value mapping is the only behavior on display.

## What it maps

| Output | Source | Mapping | Case |
|---|---|---|---|
| `SEX` | `DM.SEX` | `M` to `M`, `F` to `F` | insensitive |
| `SEXN` | `DM.SEX` | `M` to 1, `F` to 2 | insensitive |
| `SEXDECOD` | `DM.SEX` | `M` to `Male`, `F` to `Female` | insensitive |
| `RACE` | `DM.RACE` | direct copy | — |
| `RACEN` | `DM.RACE` | `WHITE` to 1, `BLACK OR AFRICAN AMERICAN` to 2, `ASIAN` to 3 | sensitive |

All four mappings use the `mapping` operation, which returns `dict[value]` and
raises `unmapped` for a value the dictionary does not contain.

One source variable feeds three output columns with three different
dictionaries, which is the whole of one-to-many variable mapping. The mappings
also change type: `SEXN` reads a `str` and produces an `int`, converted by R005
after the last operation.

`SEX` maps `M` to `M`, which looks like a no-op and is not. It is the
case-standardization step: subject `CATH-702-006` reports a lowercase `m`, and a
direct copy would carry that lowercase value into the output.

`SEXDECOD` shows the decode direction, turning a stored code into the text a
listing displays. Its name and the values `Male` and `Female` are
sponsor-defined; CDISC fixes `M` and `F`, not their display forms. Numeric
companion codes such as `SEXN` and `RACEN` are sponsor-defined for the same
reason.

## Case-insensitive matching

`case_sensitive` defaults to `true`. Setting it `false` folds `A`-`Z` to `a`-`z`
on both the input and every `dict` key before comparing, and nothing else.

The ASCII restriction is deliberate. `tolower` in R and `str.lower` in Python
return different results for `U+0130`, and Python's own `str.lower` and
`str.casefold` differ on `U+00DF`, so a Unicode or locale-aware rule would let
the same specification produce different output in each implementation. CDISC
controlled terminology is ASCII, so the restriction costs nothing here.

Two `dict` keys that fold to the same value are an error rather than
last-one-wins, so `M` and `m` cannot both appear in a case-insensitive
dictionary.

`RACEN` is left case-sensitive so both settings are visible in one fixture.

## Undefined values

`MULTIPLE` is not in the `RACEN` dictionary, so `mapping` raises `unmapped` and
the derivation substitutes 99. Without that exception the run fails, which is
the correct default: an unmappable value is normally a data-management query
rather than something to pass through silently.

## Relation to other fixtures

`sdtm-dm-basic` also maps values, translating collected `Male` and `Female` into
SDTM `M` and `F`. It standardizes collected data into SDTM terminology, while
this fixture derives ADaM companions from terminology that is already
standardized.

`sdtm-ae-dictionary-coding` performs the same kind of value mapping with the
dictionary held in an external file rather than inline, using `mapping_from`.

## Scope

This fixture was reduced from a broader mapping taxonomy that also covered
string parsing, range banding, first-non-missing, conditional flags, and
aggregate-then-join. Those behaviors remain registered in `operations.yaml` but
are no longer exercised by any fixture, and the reduction removed the only
fixture that justified `derivation.filter`, the `aggregate` kind, and R001's
predicate-dependency rule. See `../README.md` for the current coverage gap.
