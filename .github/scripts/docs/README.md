# Site pages

The home page (`/`) reads the root `README.md`; the rules index
(`/articles/rules/`) reads `rules/README.md`. The MkDocs hook renders those
sources as virtual pages, resolves their relative Markdown links to repository
files, and points their Edit and View buttons to the canonical source. There
are no Markdown copies to synchronize or commit.

Install and test with the existing pinned documentation dependencies:

```sh
python -m pip install -r docs/requirements.txt -r .github/scripts/benchmark-docs/requirements.txt
make docs-test
make docs
```

Run `make help` from the repository root for all supported targets and prerequisites.
Set `DOCS_PYTHON` when the pinned docs dependencies are installed in a separate
interpreter. CI calls the same owned `.github/scripts/commands/docs.py` entry points.

Deployment also renders the Quarto slide decks before building MkDocs. The
canonical-page tests build a small site with the real hook and Material theme;
they need neither benchmark fixtures nor Quarto. Ordinary pages under `docs/`
keep their usual site-relative links.
