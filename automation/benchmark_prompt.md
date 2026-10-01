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

- **Which benchmarks.** Every positive `sdtm-*` and `adam-*` benchmark,
  whatever its README lifecycle; a `draft` benchmark's prompt changes with
  it. `schema-*` benchmarks exercise the specification language and
  `negative-*` benchmarks have no dataset to deliver, so neither gets a
  prompt.
- **What is graded.** Every golden dataset the benchmark's specifications
  write, several for domains built together (DM and SUPPDM). A benchmark
  whose golden is a warning or violation log (`adam-adsl-age-quality`) is
  not built yet. A generated document (`define.xml`) is not graded; only
  its datasets are. A project function (`environment.yaml`) is the
  agent's to compute: state what the value is, as for any other column.
- **Where.** `benchmarks/<name>/prompt.md`, beside the README.
- **Worked examples.** `adam-adsl-age-group` (categories and codes),
  `adam-adae-death` (two inputs, precedence, ties), and `adam-adtte-dor`
  (three inputs, event and censoring rules, source tracing).

## 2. What the agent sees

| Sees | Never sees |
|---|---|
| the system prompt for the run's language (R or Python), then `prompt.md`, verbatim | `README.md`, `spec*.yaml`, `expected/` |
| `/app/input/`: the `input/` data files | `run.py`, `run.R`, yamaa itself, input schemas (`*.schema.yaml`) |
| Python and R with common data packages | the internet (web search is off) |

The agent, model, and model provider are chosen when the evaluation runs,
and the language is fixed by the task's system prompt (`system-r.md` or
`system-python.md`, which also requires `result.R` or `result.py`), so the
same benchmark prompt must work unchanged for either language. The prompt plus
the input files must be enough to reproduce every golden cell. The grader
also reruns the script from a clean state and grades what it writes.

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
- every key column, including how a sequence number runs (per subject or
  across the study, in which order, from which start): rows are matched
  by key, so a key numbered another way loses the whole row;
- the literal values of any text column the golden fills from a lookup or
  a fixed label, such as visit names (`VISIT 1`, `DAY 1`);
- a conversion factor or constant exactly as the golden uses it (`0.0167`,
  `0.45359237 kg per pound`), never "the standard factor";
- the precision of a rounded result, and that a result is not rounded
  when the golden keeps full precision (`AVAL is not rounded`);
- the text form of a number or datetime the golden holds in a text column,
  which is compared exactly (`whole numbers are written without a decimal
  point`, `ISO 8601 with a T between date and time`);
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
- describe how processing fails or stops ("stops the run", "is an error",
  "no output is written"), or the order of internal steps ("once every
  row exists", "calculated a second way"): the golden is a completed
  dataset, and only its values count;
- mention sample data: subject identifiers, row counts, or particular
  dates;
- name a programming language, package, agent, model, or provider, or
  describe the sandbox;
- prescribe file formatting, types, or sort order, or explain
  floating-point summation order. The grader matches rows by key and reads
  an empty cell, `NA`, or `.` as no value. In number and date columns it
  compares numbers as numbers within a relative 1e-9 (`1.0` equals `1`) and
  reads dates as `YYYY-MM-DD` (a midnight datetime too). Text columns are
  compared exactly, surrounding spaces included.

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
3. **Check the keys.** Number every key column the way the prompt says
   and confirm it gives the golden's keys row for row.
4. **Remove what no golden cell needs.**
5. **Agree with the README.** Every `Variables:` bullet and the `Note:`
   show up in the prompt, and nothing contradicts them.
6. **Keep it short.** Plain text with simple lists, straight quotes around
   literal values, prose wrapped at 79 columns, under about 40 lines.
7. **Solve it from the prompt.** Write the solution in R and in Python
   working only from `prompt.md` and the inputs, with only the packages the
   track's system prompt (`evaluations/harbor/system-*.md`) lists, and save
   it as `evaluations/harbor/solutions/<benchmark>/result.R` and
   `result.py`. It must score 1; if it cannot without the README, the
   prompt is missing a rule. Harbor's oracle agent then runs it, so the
   task page shows a readable reference instead of a script that writes
   the golden files. Follow the pilots' solutions for style.

## 6. Keep it in step

Change `prompt.md` in the same change as any edit to that benchmark's
README, inputs, or golden files.

## Checks to run before finishing

Run from the repository root. The second check uses the evaluation's own
build, so it checks exactly the datasets that are graded; both print
nothing but the one benchmark that is not built yet
(`adam-adsl-age-quality`).

```bash
awk 'length > 79 { print FILENAME ":" FNR }' benchmarks/*/prompt.md
```

```bash
uv run --project python --no-sync pytest python/tests/test_harbor_evaluation.py \
	-k reference_solution
```

The reference test grades every solution in `evaluations/harbor/solutions/`;
an R one is skipped when R lacks a package its script loads.

```bash
uv run --project python --no-sync python - <<'PY'
import importlib.util
import re
from pathlib import Path

path = "evaluations/harbor/build.py"
spec = importlib.util.spec_from_file_location("build", path)
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)
banned = re.compile(
    r"\b(?:yamaa|yaml|spec|specifications?|schemas?|handlers?|verifications?"
    r"|derivations?)\b|\bR0\d\d\b|\bREQ-\d+",
    re.IGNORECASE,
)
for path in sorted(Path("benchmarks").glob("*/prompt.md")):
    text = path.read_text()
    flat = " ".join(text.split())
    for match in banned.finditer(text):
        print(path, "-> mentions", match.group(0))
    try:
        contract = build.contract_for(path.parent, "r")
    except build.BuildError as exc:
        print(path, "-> not built:", exc)
        continue
    for output in contract["outputs"]:
        if f"/app/output/{output['file']}" not in flat:
            print(path, "-> no output path for", output["file"])
        if ", ".join(output["columns"]) not in flat:
            print(path, "-> column list differs from", output["file"])
PY
```
