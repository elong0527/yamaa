#!/usr/bin/env python3
"""Generate self-contained example dashboards from repository fixtures."""

import argparse
import csv
import html
import json
import re
import sys
from pathlib import Path
from string import Template
from urllib.parse import quote, urlsplit

import yaml
from markdown_it import MarkdownIt


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
EXAMPLES = ROOT / "yaml/examples"
DESTINATION = ROOT / "docs/examples"
REPOSITORY = "https://github.com/elong0527/yamaa"
OUTCOMES = (
    (
        "positive",
        "Examples",
        "Each runs to completion and produces the artifact in expected/.",
    ),
    (
        "negative",
        "Anti-pattern",
        "Each must fail, and expected/error.yaml pins the error it raises.",
    ),
)
YAML_TOKEN = re.compile(
    r'''"(?:[^"\\]|\\.)*"|'(?:[^']|'')*'|\b(?:null|true|false)\b|\b\d+(?:\.\d+)?\b|[A-Za-z_][\w-]*(?=:)'''
)
SPEC_FILE_PATTERN = re.compile(r'^spec(?:_[a-z][a-z0-9_]*)?\.yaml$')
SPEC_RESOLVED_NAME = 'spec_resolved.yaml'


def escape(value):
    return html.escape(str(value), quote=True)


def example_spec_files(example):
    if not example.is_dir():
        return []
    return sorted(
        path
        for path in example.iterdir()
        if path.is_file() and SPEC_FILE_PATTERN.fullmatch(path.name)
    )


def example_has_spec(example):
    return bool(example_spec_files(example))


def example_entry(example):
    files = example_spec_files(example)
    if (example / 'spec.yaml').is_file():
        return example / 'spec.yaml', []
    specs = {}
    for path in files:
        try:
            specs[path.resolve()] = yaml.safe_load(path.read_text(encoding='utf-8'))
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
    parented = set()
    for spec in specs.values():
        if not isinstance(spec, dict):
            continue
        parents = spec.get('parents', [])
        if isinstance(parents, str):
            parents = [parents]
        for parent in parents:
            if isinstance(parent, str) and parent:
                parented.add(Path(parent).name)
    entries = [path for path in files if path.name not in parented]
    entry = entries[0] if entries else (files[0] if files else None)
    chain = []
    if entry is not None:
        seen = set()

        def visit(path):
            if path in seen:
                return
            seen.add(path)
            spec = specs.get(path)
            parents = spec.get('parents', []) if isinstance(spec, dict) else []
            if isinstance(parents, str):
                parents = [parents]
            for parent in parents:
                if not isinstance(parent, str) or not parent:
                    continue
                candidate = path.parent / parent
                if candidate.is_file():
                    visit(candidate.resolve())
            if path != (entry.resolve() if entry else None):
                chain.append(path)
        visit(entry.resolve())
    return entry, chain


def render_readme(text, source_url):
    """Render Markdown without executing raw HTML; resolve fixture-relative links."""
    markdown = MarkdownIt("commonmark", {"html": False}).enable("table")
    tokens = markdown.parse(text)
    title = "Example"
    if tokens and tokens[0].type == "heading_open" and tokens[0].tag == "h1":
        title = tokens[1].content
        tokens = tokens[3:]
    for token in tokens:
        for child in token.children or []:
            attribute = "href" if child.type == "link_open" else "src" if child.type == "image" else None
            if attribute:
                target = child.attrGet(attribute)
                if target and not urlsplit(target).scheme and not target.startswith(("#", "//")):
                    child.attrSet(attribute, source_url + "/" + target)
            # Keep generated pages self-contained: show image references as links.
            if child.type == "image":
                child.type = "html_inline"
                child.content = f'<a href="{escape(child.attrGet("src"))}">{escape(child.content or "Image")}</a>'
    return title, markdown.renderer.render(tokens, markdown.options, {})


def subject_key(record):
    subject = record.get("USUBJID", "")
    if not subject:
        return ""
    return json.dumps([record.get("STUDYID", ""), subject], ensure_ascii=True, separators=(",", ":"))


def fixture_files(directory):
    if not directory.is_dir():
        return []
    return sorted(
        (path for path in directory.rglob("*")
         if path.is_file() and not path.is_symlink()
         and path.resolve().is_relative_to(directory.resolve())
         and not any(part.startswith(".") for part in path.relative_to(directory).parts)),
        key=lambda path: path.relative_to(directory).as_posix(),
    )


def read_csv(path):
    # Decode for display without changing field values.
    with path.open(encoding="utf-8-sig", newline="") as stream:
        rows = list(csv.reader(stream, strict=True))
    if not rows:
        return [], []
    if any(len(row) != len(rows[0]) for row in rows[1:]):
        raise ValueError("unequal CSV record widths")
    return rows[0], rows[1:]


def render_table(headers, rows, filename, derived, labels):
    cells = []
    for name in headers:
        class_name = ' class="derived"' if name in derived else ""
        title = f' title="{escape(labels[name])}"' if name in labels else ""
        cells.append(f'<th scope="col"{class_name}{title}>{escape(name)}</th>')
    body = []
    for values in rows:
        record = dict(zip(headers, values))
        key = subject_key(record)
        row_cells = []
        for name, value in zip(headers, values):
            class_name = ' class="derived"' if name in derived else ""
            content = escape(value)
            if value == "":
                content = '<span class="missing" aria-label="Missing value">&mdash;</span>'
            elif name == "USUBJID" and key:
                content = (
                    f'<button type="button" class="subject-button" data-subject="{escape(key)}" '
                    f'aria-pressed="false" aria-label="Highlight subject {escape(value)}">{content}</button>'
                )
            row_cells.append(f'<td{class_name} data-value="{escape(value)}">{content}</td>')
        body.append(f'<tr data-subject="{escape(key)}">' + "".join(row_cells) + "</tr>")
    empty = '<p class="no-rows">This file has no data rows.</p>' if not rows else ""
    return (
        f'<div class="table-scroll" tabindex="0" role="region" aria-label="{escape(filename)} table">'
        f'<table class="data-table"><caption class="visually-hidden">{escape(filename)}</caption>'
        '<thead><tr>' + "".join(cells) + '</tr></thead><tbody>' + "".join(body)
        + "</tbody></table></div>" + empty
    )


def render_files(paths, group, example, derived, labels):
    panes, widths, subjects = [], [], set()
    row_count = 0
    for index, path in enumerate(paths):
        pane_id = f"{group}-file-{index}"
        filename = path.relative_to(example).as_posix()
        count = ""
        table = None
        width = 1
        if path.suffix.lower() == ".csv":
            try:
                headers, rows = read_csv(path)
                if not headers or len(set(headers)) != len(headers) or "" in headers:
                    raise ValueError("missing or duplicate CSV header")
                table = render_table(headers, rows, filename, derived, labels)
                width = len(headers)
                row_count += len(rows)
                count = f'{len(rows)} row' + ("" if len(rows) == 1 else "s")
                for values in rows:
                    key = subject_key(dict(zip(headers, values)))
                    if key:
                        subjects.add(key)
            except (ValueError, csv.Error, UnicodeError):
                # Malformed inputs are intentional in negative examples.
                # Show their source literally instead of repairing or dropping it.
                count = "raw CSV"
        if table is None:
            try:
                table = f'<pre class="plain-file"><code>{escape(path.read_text(encoding="utf-8"))}</code></pre>'
            except UnicodeError:
                table = '<p class="no-rows">Binary fixture; inspect it in the example source.</p>'
            if count == "raw CSV":
                table = '<p class="no-rows">Showing raw CSV because the source is not a valid rectangular table.</p>' + table
        widths.append(f"minmax(0, {width}fr)")
        count_html = f'<span class="file-count">{count}</span>' if count else ""
        panes.append(
            f'<div id="{pane_id}" class="panel file-pane" role="region" aria-label="{escape(filename)}">'
            f'<div class="file-heading"><h3 class="filename">{escape(filename)}</h3>{count_html}</div>'
            + table + "</div>"
        )
    content = "".join(panes) or '<p class="no-rows">No fixture files in this directory.</p>'
    content = f'<div class="files-grid" style="--dataset-columns: {" ".join(widths)}">{content}</div>'
    return content, row_count, subjects


def highlight_yaml(line):
    result, cursor = [], 0
    for token in YAML_TOKEN.finditer(line):
        result.append(escape(line[cursor:token.start()]))
        value = token.group()
        kind = "string" if value[0] in "\"'" else "key" if line[token.end():].startswith(":") else "literal"
        result.append(f'<span class="yaml-{kind}">{escape(value)}</span>')
        cursor = token.end()
    result.append(escape(line[cursor:]))
    return "".join(result)


def example_category(name, title, spec):
    if name.startswith("spec-"):
        return "Specification", title
    category, separator, heading = title.partition(": ")
    if not separator:
        return str(spec.get("domain", "YAMAA example")), title
    return category, heading


def describe_example(example):
    """Return the page title and category without rendering fixtures."""
    source_url = REPOSITORY + "/blob/main/yaml/examples/" + quote(example.name)
    readme_path = example / "README.md"
    entry_path, _ = example_entry(example)
    if entry_path is None:
        raise ValueError(f"example has no spec file: {example.name}")
    spec_path = entry_path
    title, _ = render_readme(readme_path.read_text(encoding="utf-8"), source_url)
    spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
    spec = spec if isinstance(spec, dict) else {}
    category, _ = example_category(example.name, title, spec)
    return title, category


def page_link(name, label, relation):
    if name is None:
        return f'<span class="is-disabled" aria-disabled="true">{label}</span>'
    return f'<a href="{escape(name)}.html" rel="{relation}">{label}</a>'


def plural(count, noun):
    return f"{count} {noun}" + ("" if count == 1 else "s")


def render_index(entries):
    """Render the gallery page linking every generated dashboard.

    A specification the design must refuse is a different contract from one
    that must produce an artifact, so the two are listed apart rather than
    interleaved by domain. A `negative-` directory name is what marks the
    second group, the same test the repository validator applies.
    """
    outcomes = {key: {} for key, _, _ in OUTCOMES}
    for name, title, category in entries:
        outcome = "negative" if name.startswith("negative-") else "positive"
        outcomes[outcome].setdefault(category, []).append((name, title))
    blocks, jumps = [], []
    for key, heading, note in OUTCOMES:
        groups = outcomes[key]
        if not groups:
            continue
        total = sum(len(items) for items in groups.values())
        jumps.append(
            f'<a href="#{key}">{escape(heading)} ({total})</a>'
        )
        sections = []
        for category in sorted(groups):
            items = "".join(
                f'        <li><a href="{escape(name)}.html">{escape(title)}</a></li>\n'
                for name, title in sorted(groups[category])
            )
            sections.append(
                f'      <section aria-label="{escape(heading)}: {escape(category)}">\n'
                f"        <h3>{escape(category)}</h3>\n"
                f'        <p class="count">{plural(len(groups[category]), "example")}</p>\n'
                f"        <ul>\n{items}        </ul>\n      </section>"
            )
        blocks.append(
            f'    <section class="outcome" id="{key}" aria-labelledby="{key}-heading">\n'
            f'      <h2 id="{key}-heading">{escape(heading)}</h2>\n'
            f'      <p class="outcome-note">{plural(total, "example")}. {escape(note)}</p>\n'
            + "\n".join(sections)
            + "\n    </section>"
        )
    template = Template((HERE / "gallery.html").read_text(encoding="utf-8"))
    result = template.substitute(
        total=len(entries),
        source_url=REPOSITORY + "/tree/main/yaml/examples",
        summary="".join(jumps),
        sections="\n".join(blocks) + "\n",
    )
    return result.encode("ascii", "xmlcharrefreplace")


def render_spec_pane(filename, text, slug, single):
    lines = text.splitlines()
    code_lines, section_options = [], []
    for number, line in enumerate(lines, 1):
        line_id = f"yaml-line-{number}" if single else f"{slug}-line-{number}"
        code_lines.append(
            f'<span class="code-line" id="{line_id}"><span class="line-number" aria-hidden="true">{number}</span>'
            f'<span class="code-source">{highlight_yaml(line)}</span></span>'
        )
        match = re.match(r"^([A-Za-z_][\w-]*):", line)
        if match:
            section_options.append(f'<option value="{line_id}">{escape(match.group(1))}</option>')
    if single:
        return "".join(code_lines), "".join(section_options), len(lines)
    pane = (
        f'<div class="spec-pane" id="pane-{slug}" data-filename="{escape(filename)}" data-lines="{len(lines)}">'
        f'<div class="file-heading"><h3 class="filename">{escape(filename)}</h3>'
        f'<span class="file-count">{len(lines)} lines</span></div>'
        f'<pre><code>{"".join(code_lines)}</code></pre></div>'
    )
    return pane, "".join(section_options), len(lines)


def render_example(example, previous=None, next=None):
    source_url = REPOSITORY + "/blob/main/yaml/examples/" + quote(example.name)
    readme_path = example / "README.md"
    spec_path, chain = example_entry(example)
    if spec_path is None:
        raise ValueError(f"example has no spec file: {example.name}")
    title, readme = render_readme(readme_path.read_text(encoding="utf-8"), source_url)
    spec_text = spec_path.read_text(encoding="utf-8")
    # Parse metadata for labels and visual emphasis only; this does not execute the spec.
    spec = yaml.safe_load(spec_text)
    spec = spec if isinstance(spec, dict) else {}
    columns = [item for item in spec.get("columns", []) if isinstance(item, dict)]
    labels = {item["name"]: item["label"] for item in columns if "name" in item and "label" in item}
    derived = set()
    for item in columns:
        name = item.get("name")
        expression = item.get("derivation", {})
        expression = expression if isinstance(expression, dict) else {}
        source = expression.get("source")
        if isinstance(source, dict):
            source = source.get("variable")
        if name and source != f'{spec.get("base")}.{name}':
            derived.add(name)
    inputs = fixture_files(example / "input")
    outputs = [path for path in fixture_files(example / "expected") if path.name != SPEC_RESOLVED_NAME]
    input_files, _, input_subjects = render_files(inputs, "input", example, set(), {})
    output_files, output_rows, output_subjects = render_files(outputs, "output", example, derived, labels)
    subjects = sorted(input_subjects | output_subjects)
    subject_ids = [json.loads(key)[1] for key in subjects]
    subject_options = []
    for key in subjects:
        study, subject = json.loads(key)
        label = f"{study} / {subject}" if subject_ids.count(subject) > 1 else subject
        subject_options.append(f'<option value="{escape(key)}">{escape(label)}</option>')
    category, heading = example_category(example.name, title, spec)
    heading = heading[:1].upper() + heading[1:]
    metrics = [(len(inputs), "input files"), (len(subjects), "subjects")]
    if any(path.suffix == ".csv" for path in outputs):
        metrics.append((output_rows, "expected rows"))
    else:
        metrics.append((len(outputs), "expected files"))
    resolved_path = example / "expected" / SPEC_RESOLVED_NAME
    if not chain and not resolved_path.is_file():
        spec_code, section_options, spec_line_count = render_spec_pane(
            spec_path.name, spec_text, "yaml", True
        )
    else:
        panes = []
        documents = [(path.name, path.read_text(encoding="utf-8")) for path in chain]
        documents.append((spec_path.name, spec_text))
        if resolved_path.is_file():
            documents.append((SPEC_RESOLVED_NAME, resolved_path.read_text(encoding="utf-8")))
        for index, (filename, text) in enumerate(documents):
            pane, options, count = render_spec_pane(
                filename, text, Path(filename).stem, False
            )
            panes.append(pane)
            if index == len(documents) - 1:
                section_options, spec_line_count = options, count
        panes.append(f"<script>{(HERE / 'spec-panes.js').read_text(encoding='utf-8')}</script>")
        spec_code = "".join(panes)
    template = Template((HERE / "dashboard.html").read_text(encoding="utf-8"))
    result = template.substitute(
        example_name=escape(example.name), page_title=escape(title), heading=escape(heading),
        category=escape(category), description=escape(title + ": README, inputs, expected output, and YAML specification."),
        source_url=REPOSITORY + "/tree/main/yaml/examples/" + quote(example.name),
        prev_link=page_link(previous, "Previous example", "prev"),
        next_link=page_link(next, "Next example", "next"),
        metrics="".join(f"<div><dt>{label}</dt><dd>{count}</dd></div>" for count, label in metrics),
        subject_options="".join(subject_options), readme=readme,
        input_files=input_files,
        input_caption=f"{len(inputs)} source file" + ("" if len(inputs) == 1 else "s"),
        output_files=output_files,
        output_caption=f"{output_rows} expected row" + ("" if output_rows == 1 else "s") if any(path.suffix == ".csv" for path in outputs) else "Expected artifacts",
        spec_lines=spec_line_count, spec_code=spec_code,
        section_options="".join(section_options),
        styles=(HERE / "dashboard.css").read_text(encoding="utf-8"),
        script=(HERE / "dashboard.js").read_text(encoding="utf-8"),
    )
    # Stable bytes across machines, locales, time zones, and checkout locations.
    # Character references also keep generated pages compatible with repository ASCII lint.
    return result.encode("ascii", "xmlcharrefreplace")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("examples", nargs="*", help="Example directory names; defaults to existing generated dashboards")
    parser.add_argument("--all", action="store_true", help="Generate every example containing README.md and a spec file")
    parser.add_argument("--check", action="store_true", help="Fail if a selected dashboard is missing or differs; write nothing")
    parser.add_argument("--quiet", action="store_true", help="Suppress successful generation messages")
    parser.add_argument("--output-dir", type=Path, default=DESTINATION, help="Destination directory (default: docs/examples)")
    args = parser.parse_args()
    if args.all and args.examples:
        parser.error("choose --all or explicit example names")
    names = args.examples
    if args.all:
        names = [path.name for path in sorted(EXAMPLES.iterdir()) if example_has_spec(path) and (path / "README.md").is_file()]
    elif not names:
        names = [path.stem for path in sorted(args.output_dir.glob("*.html")) if path.stem != "index"]
    if not names:
        parser.error("specify an example name or --all")
    ordered = sorted(set(names))
    complete = sorted(path.name for path in EXAMPLES.iterdir() if example_has_spec(path) and (path / "README.md").is_file())
    neighbors = {name: (complete[index - 1] if index else None, complete[index + 1] if index + 1 < len(complete) else None) for index, name in enumerate(complete) if name in set(ordered)}
    write_index = args.all or (not args.examples and (args.output_dir / "index.html").is_file())
    failures = []
    entries = []
    for name in ordered:
        if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name):
            parser.error(f"invalid example name: {name}")
        example = EXAMPLES / name
        if not (example / "README.md").is_file() or not example_has_spec(example):
            parser.error(f"example must contain README.md and a spec file: {name}")
        try:
            previous, following = neighbors.get(name, (None, None))
            rendered = render_example(example, previous, following)
        except (OSError, ValueError, yaml.YAMLError) as error:
            print(f"Cannot generate {name}: {error}", file=sys.stderr)
            failures.append(name)
            continue
        entries.append((name,) + describe_example(example))
        destination = args.output_dir / f"{name}.html"
        if args.check:
            if not destination.is_file() or destination.read_bytes() != rendered:
                failures.append(name)
                print(f"Stale or missing: {destination}", file=sys.stderr)
            elif not args.quiet:
                print(f"Current: {name}")
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(rendered)
            if not args.quiet:
                print(f"Generated: {destination}")
    if write_index:
        index = render_index(entries)
        destination = args.output_dir / "index.html"
        if args.check:
            if not destination.is_file() or destination.read_bytes() != index:
                failures.append("index")
                print(f"Stale or missing: {destination}", file=sys.stderr)
            elif not args.quiet:
                print("Current: index")
        else:
            destination.write_bytes(index)
            if not args.quiet:
                print(f"Generated: {destination}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
