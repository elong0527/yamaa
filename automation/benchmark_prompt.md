# Benchmark prompt recipe

Canonical recipe for writing a benchmark's `prompt.md`: the request an AI
coding agent receives when the benchmark runs as an agent evaluation
(through Harbor). The agent gets this file and the benchmark's input
datasets, nothing else, and its output datasets are graded cell by cell
against the benchmark's golden files.

- write the request a statistician would send a statistical programmer:
  the standard, the inputs, the output and its record level, every output
  column, and the outcome of each derived variable
- say what each value is, never how to compute it
- state every rule the golden depends on that CDISC practice alone does not
  settle; drop everything no golden cell needs
- the file is committed verbatim; the evaluation copies it unchanged

## 1. Scope

- **Which benchmarks.** Positive `sdtm-*` and `adam-*` benchmarks whose
  README lifecycle is `reviewed` or `finalized`. `schema-*` benchmarks
  exercise the specification language and `negative-*` benchmarks have no
  dataset to deliver, so neither gets a prompt.
- **Not yet.** Benchmarks whose golden includes warning or violation logs,
  that call a project function (`environment.yaml`), or that generate a
  document (`define.xml`) wait until the evaluation supports them.
- **Where.** `benchmarks/<name>/prompt.md`, beside the README.
- **Worked examples.** `adam-adsl-age-group` (categories and codes),
  `adam-adae-death` (two inputs, precedence, ties), and `adam-adtte-dor`
  (three inputs, event and censoring rules, source tracing).

## 2. What the agent sees

| Sees | Never sees |
|---|---|
| the system prompt for the run's language (R or Python), then `prompt.md`, verbatim | `README.md`, `spec*.yaml`, `expected/` |
| `/app/input/`: the `input/` files | `run.py`, `run.R`, yamaa itself |
| Python and R with common data packages | the internet (web search is off) |

The agent, model, and model provider are chosen when the evaluation runs,
and the language is fixed by the task's system prompt (`system-r.md` or
`system-python.md`, which also requires `result.R` or `result.py`), so the
same benchmark prompt must work unchanged for either language. The prompt plus
the input files must be enough to reproduce every golden cell.

## 3. Structure

Four blocks, in this order:

```text
Following CDISC <SDTM|ADaM> standards, use the provided <A> and <B>
datasets to create an <DOMAIN> dataset with one record per <record level>.

The output dataset should contain the following columns in this order:
<every column of the golden file, in golden order>

<outcome block: labels, codes, rules, precedence, and ties>

Read the source datasets from /app/input and save the completed dataset as
/app/output/<golden file name>.
```

- **Inputs.** Name each input dataset by its uppercase identifier from the
  specification's `input` (DM, AE, ADRS_RAW).
- **Record level.** Take it from the README goal and the specification
  keys: one record per subject, per adverse event, per responder, per
  subject per parameter per visit.
- **Columns.** List every column of the golden header, pass-through
  columns included, in golden order.
- **Output file.** Use the golden file name under `/app/output/`. A
  benchmark with several golden datasets (`spec_<domain>.yaml`) names each
  dataset with its own column list and file.

## 4. The outcome block

Write it from the README's `Variables:` list and `Note:`, then confirm each
statement against the golden file.

State:

- the exact literal values the golden holds: category labels, flag values
  (`Y`), fixed `PARAMCD` and `PARAM`, and descriptive text, spelled as in
  the golden (`TUMOUR`, not `TUMOR`);
- the mapping of a numeric code to its label (`1 for <18, 2 for 18-64`);
- the precision of a rounded result when the golden is rounded;
- study-specific rules a CDISC-literate programmer cannot infer: what
  counts as the event, when and where a record is censored, which source
  wins when several could supply a value, and how ties break;
- what a derived variable holds when its inputs do not support a value,
  but only where the golden fixes an outcome the labels do not imply
  (`AGEGR1N has no value when AGEGR1 is Missing`);
- the meaning of any term a rule depends on when a programmer could read
  it two ways (a responder; an evaluable assessment).

Do not:

- mention yamaa, YAML, specifications, rule IDs, or operation names, or use
  the words *handler*, *verification*, *derivation*, or *schema* (*derive*
  as a verb is fine);
- give an algorithm, pseudo-code, join, or code;
- describe pass-through columns or ask for them to be carried or copied:
  the column list already asks for them;
- add a generic missing-value, imputation, or traceability footer;
- mention sample data: subject identifiers, row counts, or particular
  dates;
- name a programming language, package, agent, model, or provider, or
  describe the sandbox;
- prescribe file formatting, types, or sort order. The grader matches rows
  by key, reads an empty cell or `NA` as no value, compares numbers as
  numbers, and reads dates as `YYYY-MM-DD`.

Write "has no value" for an empty golden cell.

## 5. Fairness review

Before committing, review the prompt as the agent would read it:

1. **Cover every golden cell.** Walk each derived column of the golden
   file row by row. Every value must follow from the prompt, the inputs,
   and ordinary CDISC practice. A value that needs an unstated choice
   needs a sentence.
2. **Rule out a second reading.** If another plausible interpretation
   changes any golden cell, say which one applies. In `adam-adtte-dor`,
   censoring at "the last evaluable assessment before new therapy" would
   move one subject's date, so the prompt says evaluable assessments count
   whether they fall before or after new therapy.
3. **Remove what no golden cell needs.**
4. **Agree with the README.** Every `Variables:` bullet and the `Note:`
   show up in the prompt, and nothing contradicts them.
5. **Keep it short.** Plain text with simple lists, straight quotes around
   literal values, prose wrapped at 79 columns, under about 40 lines.

## 6. Keep it in step

Change `prompt.md` in the same change as any edit to that benchmark's
README, inputs, or golden files.

## Checks to run before finishing

Run from the repository root; both print nothing when the prompts are
clean.

```bash
awk 'length > 79 { print FILENAME ":" FNR }' benchmarks/*/prompt.md
```

```bash
python3 - <<'PY'
import glob, os, re
banned = ("yamaa", "yaml", "spec", "schema", "handler", "verification",
          "derivation")
for path in sorted(glob.glob("benchmarks/*/prompt.md")):
    folder = os.path.dirname(path)
    text = open(path).read()
    flat = " ".join(text.split())
    for golden in sorted(glob.glob(f"{folder}/expected/*.csv")):
        name = os.path.basename(golden)
        columns = ", ".join(open(golden).readline().strip().split(","))
        if f"/app/output/{name}" not in flat:
            print(path, "-> no output path for", name)
        if columns not in flat:
            print(path, "-> column list differs from", name)
    for word in banned:
        if re.search(rf"\b{word}\b", text, re.IGNORECASE):
            print(path, "-> mentions", word)
PY
```
