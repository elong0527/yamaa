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

## No content hashing

The repository does not hash content. A runtime artifact is pinned by
`runtime.artifact.reference` alone, a contract fingerprint is its canonical
RFC 8785 JSON rather than a hash of it, the activation cache key is the
joined identities themselves, and a resource snapshot compares the bytes it
already holds. `migration.yaml` records provenance without digests.

Do not reintroduce SHA-256 (or any digest field) to give something an
identity. Compare the canonical form directly, and keep the bytes when
equality has to be decided on bytes.

The one exception is `python/uv.lock`, whose hashes belong to the packaging
tool and are not the repository's own provenance.
