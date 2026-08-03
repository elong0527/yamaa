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
Rscript yaml/tests/R/run_parity.R           # execute in R, diff vs expected AND vs Python
python yaml/tests/python/test_negative.py   # every negative fixture must be rejected
Rscript yaml/tests/R/run_negative.R         # the same corpus, against R
```

Python needs `pyyaml`; R needs `yaml` and `jsonlite`. The parity harness shells
out to `emit_json.py`, so a difference it reports is a real disagreement between
the two implementations rather than a reporting artifact.

## Layout

| Path | Purpose |
|---|---|
| `python/validate.py` | Conformance: R006 notation, both registries, structural clauses of R001, R002, R003, R005 |
| `python/engine.py` | Predicate evaluation, type conversion, serialization, and the operation implementations |
| `python/runner.py` | Row construction, dependency ordering, joins, ODM context resolution |
| `python/test_fixtures.py` | Runs every fixture and diffs against its expected CSV |
| `R/validate.R` | The R conformance implementation |
| `R/engine.R` | Predicates, conversion, serialization, operations, host functions |
| `R/runner.R` | The R execution engine |
| `R/run_tests.R` | The R validation entry point |
| `R/functions.R` | Host functions reachable from `call`, mirroring `python/functions.py` |
| `R/run_parity.R` | Executes in R and diffs against both the CSV and Python |
| `negative/` | One fixture per error condition, with the message each must produce |
| `python/test_negative.py`, `R/run_negative.R` | Both implementations against that corpus |

## Status

Both implementations validate and **execute** all six fixtures. Every cell
matches the expected CSV, and every cell matches between R and Python. That is
the first actual evidence for the equivalence requirement in `../README.md`;
before the parity harness existed the claim was prose.

Two checks guard against the failure mode where something is registered and then
silently does nothing. `check_implemented()` fails if any registered operation
has no implementation, if an operation is implemented under the wrong `kind`, or
if an exception binds to a stage the engine ignores. That check would have caught
both `call` and `override` shipping broken.

Conformance parity is now tested too. `negative/` holds 20 fixtures, one per
error condition, each with the message it must produce, and both validators are
run against all of them. Comparing error *messages* rather than accept/reject is
deliberate: a validator that rejects a specification for the wrong reason is
still wrong, and it misleads whoever has to fix the specification.

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

## What the negative corpus caught immediately

Four places where the two validators disagreed, found mechanically rather than by
inspection.

- `source` and `literal` together: R accepted it, so a derivation with two seeds
  was legal in one implementation.
- An operation nested where an argument value belongs: R had no check.
- An unknown `call` function: R had no host-library check. Host functions moved
  into `R/functions.R`, mirroring `python/functions.py`, so both sides check the
  same list.
- An unregistered operation: R crashed with a subscript error instead of
  reporting it. Same `x[[i]] <- NULL` deletion that broke the engine, this time
  in the validator.

The R validator also never applied the variable pattern from `schema.yaml`, so
`source: 9NOTAVAR` was accepted. Both now reject it.

## What parity caught immediately

Two defects that only a second implementation could surface.

**`x[[i]] <- NULL` deletes a list element in R** rather than storing missing, so
every operation returning missing silently shortened the value list and the run
died with a subscript error. Five assignment sites needed single-bracket
assignment instead.

**Serialization diverged on a whole number reached through floating point.**
`percent_change` produces 50 as `50.000000000000099`, which is not equal to its
own rounding, so R formatted it as `"50"` and then stripped trailing zeros to
`"5"`. Python was unaffected because it takes an integer branch first. The strip
now requires a decimal point. This was the single numerically non-trivial cell in
the suite, and it was wrong in R until the harness compared it.

## Deliberate limits

The engine implements what the rules define and raises on what they leave open,
rather than guessing. It rejects an unknown column `type` instead of inventing a
conversion, and it refuses a predicate outside the R004 core subset instead of
falling back to host-language evaluation.
