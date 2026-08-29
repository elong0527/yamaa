# Derivation examples

Each subdirectory is one example: a specification, its input data, the exact
output an implementation must reproduce, and a README describing what the
example derives.

## Required reading

Read `../agents.md` first; it governs the schema and rules this directory
exercises. Then read `README.md` here for the example index and
`plan.md` for the open design gaps and the schema work they justify.

## Example layout

    <standard>-<domain>-<subject>/
        README.md
        spec.yaml
        input/*.csv
        expected/<domain>.csv

An expected failure before a dataset is completed replaces the CSV with
`expected/error.yaml`, unless the intended artifact is useful as a forward
contract. A failure over a completed dataset carries both the expected CSV and
`expected/error.yaml`.

Name the directory for what it derives, not for the construct it uses:
`sdtm-vs-visit-study-day`, not `sdtm-vs-mapping-from`.

## The README describes data, not the specification

A reader of an example README wants to know what the output means. Write:

- a title of the form `# <STANDARD> <DOMAIN>: <what the example does>`;
- one sentence naming the input sources and the output grain, ending in a
  colon;
- one bullet per output variable, in output order, saying what its value means
  and what it holds when the inputs do not support it;
- at most one closing paragraph, for a rule that governs several variables at
  once.

Keep bullets to the variables a reader must understand. Direct key copies and
fixed values need no bullet.

**Do not name anything from the schema.** No rule IDs, no expression or field
names, no `output: false`, and none of the words *handler*, *verification*,
*derivation*, or *schema*. State the effect instead:

    - counting neither the last day of period one nor the first day of period
      two, so consecutive periods give zero
    + `date_diff` with `bounds: between`

    - A subject who never entered period two has no exposure records there
    + Its period-two right side is empty after filtering, which R003 treats as
      an absent match

**Do not describe intermediate columns.** A column declaring `output: false` is
not in the artifact and does not belong in the README.

**Do not count the sample data.** "Two of the four subjects have no exposure"
is a property of the input file; "a subject with no exposure falls back to the
planned arm" is the rule. Write the rule. Row counts, keys, and expected
handler counts are stated in `spec.yaml` and do not belong in prose.

Wrap at 79 columns. Most examples fit in under 25 lines.

## Design findings belong in the catalogue

An example that cannot express something is a design finding, and findings live
in the `Open design gaps` section of `plan.md`, grouped by root cause, so that
one limitation is stated once and names the examples that show it.

Before removing a finding from an example README, confirm the catalogue in
`plan.md` records it and names the example. If it does not, migrate it first.
Deleting the only statement of a limitation is the most common way this suite
loses information.

When a gap closes, delete its entry and renumber rather than marking it closed,
and update the references elsewhere in `plan.md`.

## The specification

- Declare `keys`, and verify what the derivation guarantees rather than what
  the sample data happens to show.
- Prefer no intermediate columns. When one is unavoidable, declare
  `output: false`; it stays available to dependents and to verifications.
- Use `rows` when row construction changes the row count. A specification whose
  input already maps one-to-one to its output usually needs columns, not row
  templates. `sdtm-vs-unit-standardization` is the deliberate exception, and it
  reorders its output to get per-test separation.
- Output column order is declaration order. Dependencies are inferred, so a
  column may be declared before the one it reads.

## Golden output

The expected file is a contract. Before committing a change to one, reproduce
it independently — read the input, apply the rule by hand or in a short
script, and compare — rather than accepting whatever the change produced.

Changing a golden file is a decision. Say in the pull request which values
moved and why, and confirm that every other example's output is untouched.

## Expected failures and blocked examples

An example whose purpose is to fix failure behavior has
`expected/error.yaml`. It is a partial structured assertion over the failure
and has these fields:

- `phase`: the evaluation phase that rejects the run;
- `condition`: a stable snake-case name for the failed condition;
- `spec_paths`: one or more specification locations implicated in the failure;
- `context`: optional structured facts such as the dataset, offending keys,
  value, match count, or verification ID.

An implementation may report additional context and may word its human-readable
message differently. The expected fields and values must match. Stack traces
and implementation-specific exception classes do not belong in this artifact.

`phase` comes from a closed list, so two examples that stop at the same point
say so the same way:

| Phase | Rejects |
|---|---|
| `validation` | the specification itself, before any data is read |
| `row_construction` | evaluating a row template |
| `derivation` | evaluating a column's expression over a row |
| `output` | output identity, once every column holds its final value |
| `verification` | a declared assertion |

A condition that an operation could have answered locally instead names the
stage R008 gives it: `bind`, `join`, `mapping`, `cut`, `extract`, `template`,
`impute`, `convert`, or `final`. Declaring the corresponding handler is then
exactly what turns the failure into a value, which is what makes the pairing
worth keeping.

`condition` names what failed rather than what the implementation raised, and
one condition keeps one name across every example that provokes it.

An expected CSV may be committed beside `error.yaml`. For a failure after the
dataset is completed, it is the dataset presented to the failing check. For an
earlier expressiveness failure, it is the intended artifact once the missing
capability exists. Neither is an accepted artifact from the current failed
run. Its README still describes the expected variables under the same data-only
contract as a positive example.

## Checks to run before finishing

    # no schema vocabulary reached the README
    grep -nE "R0[01][0-9]|output: false|handler|verification" */README.md

    # every column in the golden file is described
    python3 - <<'PY'
    import glob, os
    KEYS = {"STUDYID", "USUBJID", "DOMAIN", "SUBJID", "AESEQ", "VSSEQ",
            "LBSEQ", "RSSEQ", "ASEQ", "PARAMCD", "PARAM", "AVISIT", "VISIT",
            "RDOMAIN", "IDVAR", "QNAM"}
    for f in sorted(glob.glob("*/expected/*.csv")):
        d = os.path.dirname(os.path.dirname(f))
        rd = open(f"{d}/README.md").read()
        cols = open(f).readline().strip().split(",")
        missing = [c for c in cols if c not in rd and c not in KEYS]
        if missing:
            print(d, "->", missing)
    PY

Both print nothing when the suite is clean. Key columns and fixed domain
values are skipped because they carry no logic; anything the second check
reports is a variable the README does not explain.

## Adding an example

1. Write `spec.yaml`, the input data, and either the expected output or the
   expected error. Add an expected CSV beside an error when it makes a blocked
   or rejected result concrete.
2. Write the README to the contract above.
3. Add a row to the index table in `README.md`. Its `Derives` column is the
   README title with the standard and domain prefix removed, so the two cannot
   drift apart.
4. Record any finding it exposes as a gap in `plan.md`, or add the example's
   name to the gap that already states it.

## Before deleting an example

Some examples are the only exercise of a construct or a rule, so removing one
silently drops coverage. Check what an example uniquely covers before deleting
or merging it, and replace the coverage in the same change.

## Adding an expression

An expression enters the vocabulary when an example needs it, a negative
example fixes its failure behavior, and R and Python can implement it the same
way. Sponsor-specific algorithms stay behind `function`.

Prefer one closed grammar to an entry per operator: `compute` states the whole
numeric grammar once rather than registering `add`, `subtract`, and `divide`.

Prefer widening an existing entry to adding a new one — but the test is the
kind of value, not the saving in YAML. Widen when the new behavior returns the
same kind of value, as `date_diff`'s `bounds` still returns a count. Add an
entry when it does not: `study_day` returns an ordinal on a calendar with no
zero, and folding it into `date_diff` would have allowed `unit: week` with it.

After registering one, update every place that enumerates the vocabulary: the
input-shape audit in `../README.md`, R007's type behavior, and R008 if it
declares handlers. Then delete the gap it closed, and the open item that
justified it, from `plan.md`, which carries only remaining work.
