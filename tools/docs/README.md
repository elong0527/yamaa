# Site pages

The home page (`/`) reads the root `README.md`; the rules index
(`/articles/rules/`) reads `rules/README.md`. The MkDocs hook renders those
sources as virtual pages, resolves their relative Markdown links to repository
files, and points their Edit and View buttons to the canonical source. There
are no Markdown copies to synchronize or commit.

The requirement index (`/reference/requirements/`) and schema fields
(`/reference/schema-fields/`) pages are rendered by the same hook from `rules/`
and `yaml/` with `generate_rule_reference.py` on every build. They are not
committed and have no Edit button; edit the owning contract or schema instead.

From the repository root, install and test with the existing pinned documentation
dependencies. Set `DOCS_PYTHON=/path/to/docs-env/bin/python` to select another
interpreter for both installation and the Make targets:

```sh
"${DOCS_PYTHON:-python}" -m pip install -r docs/requirements.txt -r tools/benchmark_docs/requirements.txt
make docs-test DOCS_PYTHON="${DOCS_PYTHON:-python}"
make docs DOCS_PYTHON="${DOCS_PYTHON:-python}"
```

Run `make help` from the repository root for all supported targets and prerequisites.
`docs-build` requires generated dashboards; `docs` generates them before building.
CI calls the same owned `tools/commands/docs.py` entry points.

Deployment also renders the Quarto slide decks before building MkDocs. The
canonical-page tests build a small site with the real hook and Material theme;
they need neither benchmark fixtures nor Quarto. Ordinary pages under `docs/`
keep their usual site-relative links.
