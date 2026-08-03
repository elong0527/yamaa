# Tests

Two independent implementations of the same design. R and Python must accept
the same specifications, reject the same specifications, and produce the same
output from the same inputs. Anywhere they disagree is a defect in the rules,
not a defect in one implementation.

## Running

From the repository root:

```sh
python yaml/tests/python/test_fixtures.py   # validate and execute every fixture
Rscript yaml/tests/R/run_tests.R            # validate every fixture
```

Python needs `pyyaml`; R needs the `yaml` package.

## Layout

| Path | Purpose |
|---|---|
| `python/validate.py` | Conformance: R006 notation, both registries, structural clauses of R001, R002, R003, R005 |
| `python/engine.py` | Predicate evaluation, type conversion, serialization, and the operation implementations |
| `python/runner.py` | Row construction, dependency ordering, joins, ODM context resolution |
| `python/test_fixtures.py` | Runs every fixture and diffs against its expected CSV |
| `R/validate.R` | The R conformance implementation |
| `R/run_tests.R` | The R entry point |

## Status

Python validates and **executes** all six fixtures, matching every expected CSV
cell. R validates all six. The R execution engine is not written yet, so
equivalence is currently proven for conformance only, not for output.

## What the engine settled

Executing the fixtures forced two decisions the rules had left open, and both
are now written into R005.

**Output row order.** Every expected CSV was key-sorted, but no rule said so,
and R001 orders construction by `rows` specification order. For an ADLB with one
row template per parameter those differ: construction emits every ALT record
before any ALTSI record, while the fixture expects them interleaved by subject.
R005 now orders output rows by `keys`, which made all six fixtures agree.

**Serialization.** Comparing against a CSV needs a shared rendering of integers,
floats, missing, and booleans. R005 records the convention and notes that
precision and `date` rendering remain open with the type vocabulary.

## Deliberate limits

The engine implements what the rules define and raises on what they leave open,
rather than guessing. It rejects an unknown column `type` instead of inventing a
conversion, and it refuses a predicate outside the R004 core subset instead of
falling back to host-language evaluation.
