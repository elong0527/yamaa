# Short Spellings

**Goal:** show the short spellings that are the only way to write a
plain value: a bare column reference, a bare input path, a bare ordering
value, a lone condition, a lone label string, a lone summary formula and
a plain list of checked columns. Each runs exactly as its full form.

**Input:** one `spec.yaml` over a demographics file (one record per
subject: sex and age, read as stored) and a weight file (several dated
weight records per subject, with typed sequence and weight).

**Variables:**

- `SEX`: the subject's recorded sex; a subject without one stops the run.
- `AGE`: the subject's age in years, converted from the collected text;
  blank when not collected.
- `AGE65FL`: `Y` for subjects aged 65 or older; blank otherwise. A
  lone condition needs no explicit yes or no text: it answers `Y`
  when true and stays blank when false or unknown.
- `SUBJLBL`: a readable label joining the subject identifier and sex,
  written as one plain string with placeholders; no two subjects share
  a label.
- `WTMAX`: the largest collected weight across the subject's weight
  records, written as one plain summary formula.
- `LASTWT`: the weight from the subject's latest dated record; the
  latest record wins because a bare ordering value sorts
  earliest-first.

**Note:** each short spelling is the only way to write its value and
expands to its full form before the run: a bare reference reads the
named column, a bare path names the input file, a bare ordering value
sorts earliest-first, a lone condition answers `Y` when true and stays
blank otherwise, a lone label string keeps its placeholders, a lone
formula summarizes the matched records, and a plain list names the
columns a check covers. The full form is written only when it adds
something, such as typed fields, explicit answers or a check name; a
full form that holds nothing beyond its short spelling is refused, and
one check or one key column is still written as a one-item list.
