# Conventions

## The project name is written `yamaa`, never `YAMAA`

Lower case, always, including at the start of a sentence, in headings, in page
titles, in nav labels, and in doc comments. The hex logo spells it that way and
the site is built to match. Do not "fix" it to title case or upper case.

Two places keep capitals on purpose, because the string there is not the
project name being written as prose:

- **`YAMAA-01` in `benchmark/**/input/` and `benchmark/**/expected/`** is a
  CDISC `STUDYID` data value, and uppercase study identifiers are the
  convention in clinical data. It also lives inside committed `.parquet`
  fixtures, so changing it means regenerating binary test data.
- **`YAMAA_*` in `R/cdiscbuilder/`** (`YAMAA_GRAMMAR_CONTRACTS`,
  `YAMAA_PREDICATE_RESERVED`, ...) are R constant identifiers, where
  SCREAMING_SNAKE_CASE is the whole-identifier convention.

## The two halves of the site share one palette

`docs/stylesheets/extra.css` styles the Material pages and
`.github/scripts/benchmark-docs/dashboard.css` styles the self-contained
benchmark dashboards. They are separate files on purpose -- a dashboard opens
from disk with no network request -- but they are meant to render as one site.
Change a color or a bar treatment in one and change it in the other.

## Editing a benchmark runtime file changes a pinned digest

Files under `benchmark/*/python/runtime/` and
`python/tests/projects/*/runtime/` are hashed into the `runtime.artifact.digest`
of the sibling `environment.yaml`. Editing one -- even a comment -- fails
execution with `runtime_artifact_mismatch` until the digest is updated.
Recompute it with `yamaa.functions.artifact.artifact_digest(Path(runtime_dir))`
and write the new value into every `environment.yaml` that recorded the old one
(the same artifact can be pinned from more than one place).
