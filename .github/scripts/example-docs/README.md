# Example dashboards

This iteration creates one self-contained HTML example:
`docs/examples/adam-adae-death-outcome.html`. It is generated from `README.md`,
`spec.yaml`, and regular files under `input/` and `expected/`. Edit source fixtures or the shared
template, stylesheet, and script here, then regenerate; do not edit generated
HTML manually.

From the repository root, generate the pilot:

```sh
uv run --with-requirements .github/scripts/example-docs/requirements.txt python .github/scripts/example-docs/generate.py adam-adae-death-outcome
```

With no example names, regenerate the existing dashboard in `docs/examples/`.
`--output-dir PATH` changes the output directory for a temporary preview.

Check that existing dashboards match the current fixtures and template:

```sh
uv run --with-requirements .github/scripts/example-docs/requirements.txt python .github/scripts/example-docs/generate.py --check
```

Run the generator checks:

```sh
uv run --with-requirements .github/scripts/example-docs/requirements.txt python -m unittest discover -s .github/scripts/example-docs -p 'test_*.py'
```

The dependencies, including the Markdown parser's transitive dependency, are
pinned. Files and subjects are sorted, output uses fixed ASCII bytes with HTML
character references, and pages contain no timestamps, absolute checkout
paths, network responses, or generated identifiers. Identical fixtures,
templates, and dependencies produce identical HTML bytes. `--check` compares
those bytes and never writes files.

The HTML embeds its CSS, JavaScript, and full rendered content. It opens
directly from disk, works offline, and is copied unchanged by GitHub Pages
because it has no Jekyll front matter. The complete README spans the top.
The YAML specification occupies a left sidebar with a Hide Spec / Show Spec
button; hiding it gives the datasets the full width. On desktop, a drag handle
resizes the sidebar and supports the arrow, Home, and End keys. A menu inside
the sidebar jumps to top-level YAML sections. Input datasets appear side by
side, with expected output below. All datasets stay visible without tabs, and
there are no downloads. Small screens stack the layout. Content remains
readable with JavaScript disabled and when printing.

The Example dashboards workflow runs on relevant pushes and pull requests. It
generates every repository example twice under different time zones and Python
hash seeds, compares the resulting directories byte for byte, checks that the
committed pilot is current, and uploads the complete generated set as the
`yamaa-example-dashboards` workflow artifact.

The generator displays expected artifacts; it does not execute YAMAA or claim
that expected output was reproduced. YAML is parsed only for display metadata.
Shading identifies output columns whose own derivation is not a direct copy of
the same-named base column. Subject highlighting matches `STUDYID` and
`USUBJID`, preventing subjects from different studies being grouped together.
Malformed CSV fixtures display as raw text; binary fixtures are identified.
Hidden files and symlinks are excluded. Inherited specifications and external
inputs are shown as written, without resolving or executing them.
