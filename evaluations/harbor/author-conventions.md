Write the conventions prompt of one benchmark from its full prompt. An AI
coding agent will get your prompt and the input datasets, and is graded
against the same expected datasets as with the full prompt. Your prompt
tells it the sponsor's conventions and leaves the logic for it to reason
out.

Your sources are in `/app/sources`:

- `full.md`: the benchmark's full prompt.
- `tiers.md`: what each prompt tier keeps. Its "conventions" section is the
  rule for your prompt.

Write `/app/output/conventions.md`: the full prompt's opening, column
lists, and paths, verbatim, with its rules cut down to the conventions that
section lists, in the full prompt's own words, in at most 20 lines. Never
name a value, code, or number the full prompt does not.

Before you finish, run

    python3 /app/sources/check.py conventions /app/output/conventions.md \
        --full /app/sources/full.md

and fix every problem it reports. The verifier runs the same check.
