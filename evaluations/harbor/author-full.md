Write the full prompt of one benchmark: the request a statistician sends a
statistical programmer. An AI coding agent will later get your prompt and
the input datasets, nothing else, and must reproduce the benchmark's
expected datasets cell by cell. You never see those expected datasets, and
you do not need them: every value follows from your sources.

Your sources are in `/app/sources`:

- `README.md`: the benchmark's data contract. Start from its Variables list
  and its Note.
- `specification/`: how the expected datasets are derived, without the
  checks run on them. Take every rule the README leaves open from here and
  restate it as an outcome.
- `functions/`, when present: the project functions the derivation calls.
- `rules/`: what each construct of the derivation means, such as where
  missing values sort.
- `recipe.md`: how a full prompt is written. Its sections 3 and 4 are the
  rules for your prompt.
- `skeleton.md`: the fixed parts of your prompt.
- `previous.md` and `feedback.md`, when present: your earlier prompt, and
  what solvers who saw only it and the inputs failed to reproduce.

The input datasets are in `/app/input`, exactly as the agent will get them.

Write `/app/output/full.md`:

- Start from `skeleton.md`. Replace each `<record level>` (it may wrap
  across lines) with what one record stands for: an adverse event, a
  subject per parameter per visit. Add a definition after the opening when
  the record level needs one. Keep the rest of the opening, the column
  lists, and the paths exactly as they are.
- Replace `<rules>` with the rules: literal values, codes and the labels
  they stand for, how each key column runs, constants, precision, the text
  form of numbers and dates in text columns, what counts as an event, which
  source wins, how ties break, and what a column holds when its inputs give
  it no value. Leave out what CDISC practice already settles.
- Say what each value is, never how to compute it: no steps, joins, or
  code. Never mention yamaa, YAML, specifications, rule identifiers, or a
  name only your sources use: an intermediate result, a column the output
  does not hold, an operation. Never mention sample data: subject
  identifiers, dates, or counts from the inputs.
- Keep every line within 79 columns and the prompt under about 40 lines.

Write `/app/output/trace.json`, a JSON list with an entry for each sentence
of your rules that quotes it and names where it comes from:

    [{"prompt": "DTHDT is the start date of the fatal adverse event",
      "sources": ["README: \"the start date of the fatal event\"",
                  "spec.yaml: columns.DTHDT.derivation"]}]

A source is `README: "<quoted README text>"`; a specification file and a
dotted path to what it states, through mapping keys and the name or id of
list entries (`spec.yaml: intermediates.DEATHEV.filter`); an input file and
what it holds (`input/ae.csv: AEOUT`); or a rule file
(`rules/execution/ordering.md: missing values sort last`).

When `feedback.md` is present, revise `previous.md`: for each column it
names, find the rule your prompt is missing in your sources and state it.
Never guess a value. If your sources cannot explain a column, say so in
`/app/output/questions.md` for a person to decide.

Before you finish, run

    python3 /app/sources/check.py full /app/output/full.md \
        --data /app/sources/check.json --trace /app/output/trace.json

and fix every problem it reports. The verifier runs the same check.
