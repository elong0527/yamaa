# Benchmark dashboards

This iteration generates one self-contained HTML page per benchmark plus an
`index.md` gallery, all under `docs/benchmark/`. Each page is generated from
its `README.md`, `spec.yaml`, and regular files under `input/` and `expected/`.
Edit source fixtures or the shared templates, stylesheet, and script here, then
regenerate; do not edit generated output manually.

The gallery is Markdown, so MkDocs renders it as an ordinary page of the
documentation site, in the nav under `Benchmark`, with the site header,
navigation, search, and light and dark palettes. The dashboards stay
self-contained. Both carry the navy sampled from the hex logo and the font
stacks the Material pages fall back to, so the two halves read as one site;
`dashboard.css` and `docs/stylesheets/extra.css` hold the two copies of those
values.

From the repository root, generate every benchmark and the gallery:

```sh
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python .github/scripts/benchmark-docs/generate.py --all
```

With explicit benchmark names, regenerate only those dashboards; each keeps the
previous and next links of the full gallery, and the gallery itself is left
untouched:

```sh
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python .github/scripts/benchmark-docs/generate.py adam-adae-death-outcome
```

With no benchmark names, regenerate the dashboards already present in
`docs/benchmark/`, plus the gallery when `index.md` is among them.
`--output-dir PATH` changes the output directory for a temporary preview.

Check that existing dashboards match the current fixtures and template:

```sh
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python .github/scripts/benchmark-docs/generate.py --check
```

Run the generator checks:

```sh
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python -m unittest discover -s .github/scripts/benchmark-docs -p 'test_*.py'
```

The dependencies, including the Markdown parser's transitive dependency, are
pinned. Files and subjects are sorted, output uses fixed ASCII bytes with HTML
character references, and pages contain no timestamps, absolute checkout
paths, network responses, or generated identifiers. Identical fixtures,
templates, and dependencies produce identical bytes. `--check` compares
those bytes and never writes files.

A dashboard embeds its CSS, JavaScript, and full rendered content. It opens
directly from disk, works offline apart from comments, and is copied
unchanged by GitHub Pages because it has no Jekyll front matter. The one
file it loads is `../assets/logo.jpeg`, the logo it shares with the
documentation site, on a path that resolves both in the repository and on
the published site. The header bar around it repeats the one Material
draws, and links back to the documentation root, to the gallery, and to the
benchmark source. The README renders as the Summary panel spanning the top. Its Standard and Domain metadata
appear as the dotted context above the page title instead of being repeated in
the Summary. Summaries with no more than ten non-empty source lines use one
left-aligned column; longer summaries use two columns, keeping each section
label with the content that follows it.
The YAML specification occupies a left sidebar with a Hide Spec / Show Spec
button; hiding it gives the datasets the full width. On desktop, a drag handle
resizes the sidebar and supports the arrow, Home, and End keys. Its default
width fits 80-character YAML lines when the viewport has room. The active spec
path, Edit link, and line count share the same file-heading treatment as the
input panes; inherited benchmarks use the path menu to switch all three. Input
datasets appear side by side, with expected output below. A benchmark carrying `expected/error.yaml`
instead renders an `Expected failure` section with the rejection facts and the
assertion collapsed, a red `Rejected` result metric, and its datasets under an
`Unexpected Output` heading. All datasets stay visible without tabs, and
there are no downloads. Small screens stack the layout. Content remains
readable with JavaScript disabled and when printing.

Below the datasets, every dashboard has a Comments section powered by
[giscus](https://giscus.app). Visitors sign in with GitHub to comment or react.
The embedded reaction controls directly expose only thumbs up and thumbs down,
with a count beside each; the custom giscus theme for that focused presentation
lives at `docs/assets/giscus-yamaa.css`. Each comment and reaction is stored in this
repository's GitHub Discussions, so feedback persists across rebuilds and
deployments and can be moderated there. Each benchmark maps to one discussion
titled `COMMENT_TERM_PREFIX` plus the benchmark directory name
(`data-mapping="specific"`), so the thread follows the benchmark directory rather
than the page URL; renaming the directory starts a new thread unless the
discussion is retitled to match. The prefix is still `yaml/examples/`, the
directory name from before the move to `benchmarks/`, because changing it
would orphan every comment posted so far. The comment widget is the only
part of a page that needs the network; without it, or without JavaScript,
the rest of the page works as before and a link points to Discussions.

Comments need a one-time setup by a repository admin: enable Discussions,
create a `Comments` category of the Announcement type (so only maintainers and
giscus open threads), install the [giscus app](https://github.com/apps/giscus)
on the repository, and copy the category ID shown at https://giscus.app into
`GISCUS["category_id"]` in `generate.py`.

Every dashboard links back to the gallery from its header, beside the source
link, and again from its footer between the previous and next benchmark. The
header link is what makes the gallery reachable from a long page without
scrolling to the end of it, and the brand beside it returns to the
documentation site.

The gallery lists `Benchmarks` before `Anti-pattern`, each group headed by its
own count and subdivided by the standard and domain in the benchmark title. A
`negative-` directory name is what puts a benchmark in the second group, the
same test the repository validator applies. The two are separate contracts --
one must produce an artifact, the other must refuse to -- so the gallery does
not interleave them by domain. Links at the top of the page jump to either
group, and every group and domain heading is Markdown, so the page's table of
contents lists them. The lists themselves are written as HTML, which escapes
each title exactly once and lets `docs/stylesheets/extra.css` lay them out as
a grid. The page asks for `hide: actions` in its front matter, because it is
generated and the Edit and View buttons Material would draw for it would point
at a file that is not in the repository; `docs/overrides/partials/actions.html`
is what honors that.

The `Benchmark dashboards` workflow runs on relevant pushes and pull requests. It
generates every repository benchmark twice under different time zones and Python
hash seeds, compares the resulting directories byte for byte, checks that the
committed pages and gallery are current, and uploads the complete generated
set as the `yamaa-benchmark-dashboards` workflow artifact. MkDocs builds
`docs/benchmark/index.md` to `benchmark/index.html`, which GitHub Pages serves
as the directory index, so `index.html` in a dashboard's own links reaches the
gallery and every page is reachable from the site.

The generator displays expected artifacts; it does not execute yamaa or claim
that expected output was reproduced. YAML is parsed only for display metadata.
Shading identifies output columns whose own derivation is not a direct copy of
the same-named base column. Subject highlighting matches `STUDYID` and
`USUBJID`, preventing subjects from different studies being grouped together.
Malformed CSV fixtures display as raw text; binary fixtures are identified.
Hidden files and symlinks are excluded. Inherited specifications and external
inputs are shown as written, without resolving or executing them.
