# Judge rubrics

Stage 4 review for a proposed benchmark. Score each criterion
**pass**, **revise**, or **fail**, with one quoted piece of evidence
(a fixture row, a prompt sentence, a spec line) per non-pass. A
single **fail** rejects the revision; any **revise** sends it back to
the proposer with the evidence attached. All five **pass** and the
revision is accepted as the new checkpoint.

## 1. Construct validity

*Does the benchmark measure a real derivation pattern or a real
failure mode, distinct from what the suite already covers?*

- Pass: the pattern appears in CDISC guidance, pharmaverse/admiral,
  or production stat-programming practice, and no existing benchmark
  pins the same rule (check `benchmarks/README.md` and the
  near-duplicates named in the task spec).
- Revise: the pattern is real but the benchmark also exercises an
  unrelated second idea -- split it.
- Fail: the benchmark tests fixture trivia (e.g. a value that is
  correct only because the sample data happens to look that way), or
  duplicates an existing benchmark's rule.

## 2. Correctness

*Does the reference produce exactly the golden, and does the golden
deserve to be the contract?*

- Pass: the entry spec runs clean through the engine, its output
  matches `expected/` cell by cell, and the golden was independently
  reproduced (hand or throwaway script), not accepted from the
  engine. For a negative benchmark: the engine rejects the spec in
  the declared phase, and `expected/error.yaml` matches on `phase`,
  `condition`, `spec_paths`, and `requirement`.
- Revise: minor mismatch with a clear fix (a label, a column order,
  a missing edge-case row).
- Fail: the spec does not run, the golden contradicts the README's
  stated rule, or the golden was clearly accepted from engine output
  (values no human rule would produce).

## 3. Feasibility

*Can a competent solver produce the golden from the prompt and the
inputs alone?*

- Pass: the oracle path works -- a reference solution written from
  `full.md` plus `input/` scores 1, and the changed-input challenge
  passes (the prompt states rules, not sample facts).
- Revise: the oracle passes but only barely, or the prompt leaves a
  genuine ambiguity two reasonable readers would resolve differently
  (quote the sentence).
- Fail: the task is unsolvable as stated, or solvable only by
  guessing unstated sponsor conventions. (A benchmark can be hard;
  it cannot be unfair.)

## 4. Usefulness

*Does this benchmark earn its place in the suite?*

- Pass: it closes a named gap (`vocabulary-coverage.yaml`,
  `validation-manifest.yaml`, an open issue, or the task spec's
  acceptance sketch), and the difficulty pilot shows it
  discriminates: solvers spread across the score range rather than
  all passing or all failing for the same reason.
- Revise: the gap is real but the fixtures make it too easy (every
  pilot solver passes) -- harden with the traps from the task spec.
  Or the benchmark overlaps an existing one -- narrow the scope and
  name the line between them.
- Fail: saturated (solvers at ceiling with nothing to learn), or the
  gap it claims is already closed.

## 5. No leakage, no gaming

*Does the benchmark survive adversarial reading?*

- Pass: the prompt never names schema vocabulary, derivation verbs,
  or the spec's structure; the README's data-contract portion obeys
  the prose rules in `benchmarks/agents.md`; fixtures contain no
  row a solver could memorize into a correct answer under the
  held-out rerun (every third subject dropped) and the changed-input
  challenge.
- Revise: a prompt sentence or README bullet hands over structure
  that should be derived (quote it).
- Fail: the answer is written into the task materials -- e.g. the
  prompt restates the golden, or a fixture column is the answer
  under a thin disguise.

## Verdict format

```
Benchmark: <name> (revision <n>)
1. Construct validity: pass | revise | fail -- <evidence>
2. Correctness:        pass | revise | fail -- <evidence>
3. Feasibility:        pass | revise | fail -- <evidence>
4. Usefulness:         pass | revise | fail -- <evidence>
5. No leakage:         pass | revise | fail -- <evidence>
Verdict: accept | revise | reject
Next: <the single most important change, or "ready for the human gate">
```

Keep the evidence to one quoted item per criterion. The verdict is a
screening tool for the human gate, not a replacement for it: an
accepted benchmark still needs a human to move the lifecycle badge.

## Maintenance checks

Apply in addition to the five criteria when the work item maintains
an existing benchmark.

**No silent regression (enhance / recalibrate).** Diff the golden
before and after. Every moved value is named with its reason in the
proposal report; everything else is byte-identical. A moved value
with no reason is a **fail**.

**No coverage loss (combine / retire).** Every construct and rule
the old benchmarks pinned is still pinned: check
`vocabulary-coverage.yaml` and `validation-manifest.yaml` entries
still resolve, and the merged benchmark's fixtures still exercise
each absorbed benchmark's edge cases. Lost coverage is a **fail**.

**Prompt parity.** The Harbor prompt asks for exactly what the new
golden holds -- same datasets, same columns, same rules. A prompt
that still describes the pre-change benchmark is a **revise**.

**Retire bar.** A retire verdict of accept needs all three: a clean
`factory.sh retire-check`, a recorded reason (redundant, trivial,
or off-mission -- with the evidence), and an explicit human
decision. The judge prepares the first two; it never supplies the
third.
