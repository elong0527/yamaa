"""Generate self-contained benchmark dashboards from repository fixtures."""

import argparse
import csv
import html
import json
import re
import sys
from pathlib import Path
from string import Template
from urllib.parse import quote, urlsplit

from markdown_it import MarkdownIt

import yaml
from evaluations.harbor.brief import brief
from tools.benchmark import LIFECYCLES, load_metadata, specification_name
from tools.benchmark import entries as benchmark_entries
from tools.benchmark_docs import define_doc, mapping_doc, review_status
from tools.conformance.public_conformance import plan
from tools.paths import ROOT

HERE = Path(__file__).resolve().parent

BENCHMARKS = ROOT / "benchmarks"
DESTINATION = ROOT / "docs/benchmark"
# The gallery's prose is authored as a documentation page, beside the other
# articles, rather than as a template buried with the generator: whoever
# writes the overview edits it where the rest of the documentation lives. It
# carries substitution placeholders, so MkDocs cannot render it on its own and
# `exclude_docs` in mkdocs.yml keeps it out of the built site; the generated
# docs/benchmark/index.md is the page readers see.
OVERVIEW = ROOT / "docs/articles/benchmark.md"
# The review page is authored the same way, and only its status is generated.
REVIEW_PAGE = ROOT / "docs/articles/benchmark-review.md"
# The agent evaluation keeps each benchmark's prompts and its reference
# solutions written without yamaa, keyed by benchmark directory name. The
# dashboard shows them as assessment materials beside the comments.
HARBOR = ROOT / "evaluations/harbor"
# Most rules first, as evaluations/harbor/prompts/README.md defines the tiers.
PROMPT_TIERS = (
    ("full", "Full", "every rule an expected cell depends on"),
    ("conventions", "Conventions", "the sponsor's conventions only"),
    ("brief", "Brief", "the opening, columns, and paths only"),
)
SOLUTION_FILES = ("result.R", "result.py", "result.sas")
REPOSITORY = "https://github.com/elong0527/yamaa"
# A review is a short survey, as on the Pilot 7 dashboards: fixed choices and
# one free-text box. Submitting opens GitHub's new-discussion composer with the
# answers filled in, so the static page holds no credentials; the reviewer
# still selects Start discussion. Each review is its own discussion in the
# Comments category. A reviewer who agrees with every question approves the
# benchmark, and a disagreement needs a comment.
REVIEW_URL = REPOSITORY + "/discussions/new?category=comments"
AGREEMENT = ("Agree", "Disagree")
# Submitting or skipping a review leaves for the queue page, which holds the
# order and sends the reviewer on. It walks these gallery groups, so one dataset
# type is reviewed together. Nobody assigns benchmarks, so the page spreads
# reviewers itself. The form marks each benchmark done in the browser under this
# key, and the queue page reads the same key.
REVIEW_GROUPS = ("sdtm", "adam")
REVIEW_QUEUE_URL = "review/next/"
REVIEW_DONE_KEY = "yamaa-benchmark-reviewed:"
LIFECYCLE_URL = REPOSITORY + "/blob/main/benchmarks/README.md#lifecycle"
# This order is shared by the overview table, jump links and gallery sections.
# SEND uses the SDTM model, so its nonclinical examples join the tabulation
# group while retaining their authored standard in the individual dashboards.
GROUP_NOTES = {
    "sdtm": "Assess collected data to SDTM and SEND tabulations",
    "adam": "Assess SDTM to ADaM and ADaM to results derivations",
    "schema": "Assess yamaa schema",
    "negative": "Assess yamaa error handling",
}
YAML_TOKEN = re.compile(
    r""""(?:[^"\\]|\\.)*"|'(?:[^']|'')*'|\b(?:null|true|false)\b|\b\d+(?:\.\d+)?\b|[A-Za-z_][\w-]*(?=:)"""
)
SPEC_FILE_PATTERN = re.compile(r"^spec(?:_[a-z][a-z0-9_]*)?\.yaml$")
SPEC_RESOLVED_NAME = "spec_resolved.yaml"


def escape(value):
    return html.escape(str(value), quote=True)


def benchmark_spec_files(benchmark):
    if not benchmark.is_dir():
        return []
    return sorted(
        path
        for path in benchmark.iterdir()
        if path.is_file() and SPEC_FILE_PATTERN.fullmatch(path.name)
    )


def benchmark_has_spec(benchmark):
    return bool(benchmark_spec_files(benchmark))


def linked_specs(spec):
    """Return the files a spec names as its parent, as the fragments it
    includes, or as input producers."""
    if not isinstance(spec, dict):
        return []
    parent = spec.get("parent")
    linked = [parent] if isinstance(parent, str) and parent else []
    include = spec.get("include")
    if isinstance(include, list):
        linked.extend(path for path in include if isinstance(path, str) and path)
    inputs = spec.get("input", {})
    if isinstance(inputs, dict):
        linked.extend(
            source["schema"]
            for source in inputs.values()
            if isinstance(source, dict)
            and isinstance(source.get("schema"), str)
            and source["schema"]
        )
    return linked


def sibling_producers(benchmark, spec):
    """Return the producers the entry reads through `schema` that the benchmark
    builds itself: spec files beside the entry or other declared entries."""
    inputs = spec.get("input", {}) if isinstance(spec, dict) else {}
    built = set(benchmark_spec_files(benchmark))
    built.update(
        benchmark / call["specification"]
        for call in benchmark_entries(benchmark)["python"]
    )
    producers = []
    for source in inputs.values() if isinstance(inputs, dict) else ():
        if not isinstance(source, dict) or not isinstance(source.get("schema"), str):
            continue
        candidate = benchmark / source["schema"]
        if candidate in built:
            producers.append(candidate)
    return producers


def benchmark_entry(benchmark):
    files = benchmark_spec_files(benchmark)
    specs = {}
    for path in files:
        try:
            specs[path.resolve()] = yaml.safe_load(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, yaml.YAMLError):
            continue
    entry = benchmark / benchmark_entries(benchmark)["python"][-1]["specification"]
    chain = []
    if entry is not None:
        seen = set()

        def visit(path):
            if path in seen:
                return
            seen.add(path)
            for link in linked_specs(specs.get(path)):
                candidate = path.parent / link
                if candidate.is_file() and candidate.resolve() in specs:
                    visit(candidate.resolve())
            if path != (entry.resolve() if entry else None):
                chain.append(path)

        visit(entry.resolve())
    return entry, chain


def readme_lines(text):
    return text.splitlines()


def readme_body_line_count(text):
    lines = readme_lines(text)
    if lines and lines[0].startswith("# "):
        lines = lines[1:]
    return sum(bool(line.strip()) for line in lines)


def render_readme(text, source_url):
    """Render Markdown without executing raw HTML; resolve fixture-relative links."""
    text = "\n".join(readme_lines(text))
    markdown = MarkdownIt("commonmark", {"html": False}).enable("table")
    tokens = markdown.parse(text)
    title = "Benchmark"
    if tokens and tokens[0].type == "heading_open" and tokens[0].tag == "h1":
        title = tokens[1].content
        tokens = tokens[3:]
    return title, render_markdown_tokens(markdown, tokens, source_url)


def render_markdown_tokens(markdown, tokens, source_url):
    """Render parsed Markdown with its links resolved against `source_url`.

    Relative links and images point at the source folder on GitHub, and images
    show as links, so a page never fetches anything when opened from disk.
    """
    for token in tokens:
        for child in token.children or []:
            attribute = (
                "href"
                if child.type == "link_open"
                else "src"
                if child.type == "image"
                else None
            )
            if attribute:
                target = child.attrGet(attribute)
                if (
                    target
                    and not urlsplit(target).scheme
                    and not target.startswith(("#", "/"))
                ):
                    child.attrSet(attribute, source_url + "/" + target)
            # Keep generated pages self-contained: show image references as links.
            if child.type == "image":
                child.type = "html_inline"
                child.content = f'<a href="{escape(child.attrGet("src"))}">{escape(child.content or "Image")}</a>'
    return markdown.renderer.render(tokens, markdown.options, {})


def subject_key(record):
    subject = record.get("USUBJID", "")
    if not subject:
        return ""
    return json.dumps(
        [record.get("STUDYID", ""), subject], ensure_ascii=True, separators=(",", ":")
    )


def fixture_files(directory):
    if not directory.is_dir():
        return []
    return sorted(
        (
            path
            for path in directory.rglob("*")
            if path.is_file()
            and not path.is_symlink()
            and path.resolve().is_relative_to(directory.resolve())
            and not any(
                part.startswith(".") for part in path.relative_to(directory).parts
            )
        ),
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
                content = (
                    '<span class="missing" aria-label="Missing value">&mdash;</span>'
                )
            elif name == "USUBJID" and key:
                content = (
                    f'<button type="button" class="subject-button" data-subject="{escape(key)}" '
                    f'aria-pressed="false" aria-label="Highlight subject {escape(value)}">{content}</button>'
                )
            row_cells.append(
                f'<td{class_name} data-value="{escape(value)}">{content}</td>'
            )
        body.append(f'<tr data-subject="{escape(key)}">' + "".join(row_cells) + "</tr>")
    empty = '<p class="no-rows">This file has no data rows.</p>' if not rows else ""
    return (
        f'<div class="table-scroll" tabindex="0" role="region" aria-label="{escape(filename)} table">'
        f'<table class="data-table"><caption class="visually-hidden">{escape(filename)}</caption>'
        "<thead><tr>"
        + "".join(cells)
        + "</tr></thead><tbody>"
        + "".join(body)
        + "</tbody></table></div>"
        + empty
    )


def render_files(paths, group, benchmark, derived, labels):
    panes, widths, subjects = [], [], set()
    row_count = 0
    for index, path in enumerate(paths):
        # Transport bytes are available in the source repository; the CSV
        # companion is the reviewable data, so do not spend a pane on .xpt.
        if path.suffix.lower() == ".xpt":
            continue
        pane_id = f"{group}-file-{index}"
        filename = path.relative_to(benchmark).as_posix()
        count = ""
        table = None
        is_document = False
        width = 1
        if path.suffix.lower() == ".csv":
            try:
                headers, rows = read_csv(path)
                if not headers or len(set(headers)) != len(headers) or "" in headers:
                    raise ValueError("missing or duplicate CSV header")
                table = render_table(headers, rows, filename, derived, labels)
                width = len(headers)
                row_count += len(rows)
                count = f"{len(rows)} row" + ("" if len(rows) == 1 else "s")
                for values in rows:
                    key = subject_key(dict(zip(headers, values)))
                    if key:
                        subjects.add(key)
            except (ValueError, csv.Error, UnicodeError):
                # Malformed inputs are intentional in negative benchmarks.
                # Show their source literally instead of repairing or dropping it.
                count = "raw CSV"
        if table is None:
            try:
                text = path.read_text(encoding="utf-8")
                table = f'<pre class="plain-file"><code>{escape(text)}</code></pre>'
                preview = None
                if path.suffix.lower() in {".html", ".htm"}:
                    # Keep reports isolated and self-contained, including when
                    # a dashboard is opened from disk.
                    policy = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:"
                    document = (
                        f'<!doctype html><meta http-equiv="Content-Security-Policy" content="{policy}">'
                        + text
                    )
                    preview = (
                        f'<iframe class="html-preview" title="Rendered {escape(filename)}" '
                        f'sandbox="" srcdoc="{escape(document)}"></iframe>'
                    )
                    count = "Rendered HTML"
                elif path.suffix.lower() == ".xml":
                    preview = define_doc.render_define(text, pane_id)
                    if preview is not None:
                        count = "Define-XML"
                if preview is not None:
                    is_document = True
                    table = (
                        preview
                        + '<details class="file-source"><summary>Original source</summary>'
                        + table
                        + "</details>"
                    )
            except UnicodeError:
                table = '<p class="no-rows">Binary fixture; inspect it in the benchmark source.</p>'
            if count == "raw CSV":
                table = (
                    '<p class="no-rows">Showing raw CSV because the source is not a valid rectangular table.</p>'
                    + table
                )
        if not is_document:
            widths.append(f"minmax(0, {width}fr)")
        count_html = f'<span class="file-count">{count}</span>' if count else ""
        edit_url = (
            REPOSITORY
            + "/edit/main/benchmarks/"
            + quote(benchmark.name)
            + "/"
            + "/".join(quote(part) for part in filename.split("/"))
        )
        panes.append(
            f'<div id="{pane_id}" class="panel file-pane{" document-pane" if is_document else ""}" role="region" aria-label="{escape(filename)}">'
            f'<div class="file-heading"><span class="file-title"><h3 class="filename">{escape(filename)}</h3>'
            f'<a class="edit-button" href="{edit_url}">Edit</a></span>{count_html}</div>'
            + table
            + "</div>"
        )
    empty = (
        "No preview available; inspect the fixture files in the benchmark source."
        if paths
        else "No fixture files in this directory."
    )
    content = "".join(panes) or f'<p class="no-rows">{empty}</p>'
    columns = " ".join(widths) or "minmax(0, 1fr)"
    content = (
        f'<div class="files-grid" style="--dataset-columns: {columns}">{content}</div>'
    )
    return content, row_count, subjects


def render_failure_section(error_path, edit_url):
    """Render the expected-failure panel for a benchmark the run must reject.

    The definition list carries only the stable facts; the full assertion,
    including spec paths, lives in the collapsed raw YAML below it.
    """
    raw = error_path.read_text(encoding="utf-8")
    try:
        failure = yaml.safe_load(raw)
    except yaml.YAMLError:
        failure = None
    failure = failure if isinstance(failure, dict) else {}
    facts = []
    for key in ("phase", "condition", "rule"):
        value = failure.get(key)
        if value:
            facts.append((key, str(value)))
    facts_html = "".join(
        f"<div><dt>{escape(key)}</dt><dd>{escape(value)}</dd></div>"
        for key, value in facts
    )
    section = (
        '<section id="expected-failure" class="panel failure-panel" aria-labelledby="expected-failure-heading">\n'
        '  <header class="panel-header"><span class="panel-title"><h2 id="expected-failure-heading">Expected failure</h2>'
        f'<a class="edit-button" href="{edit_url}">Edit</a></span>'
        '<span class="panel-caption">the run is rejected; no artifact is accepted</span></header>\n'
        '  <div class="source-path"><code>expected/error.yaml</code></div>\n'
        f"  <dl>{facts_html}</dl>\n"
        "  <details><summary>expected/error.yaml</summary>\n"
        f'  <pre class="plain-file"><code>{escape(raw)}</code></pre></details>\n'
        "</section>"
    )
    return failure, section


def highlight_yaml(line):
    result, cursor = [], 0
    for token in YAML_TOKEN.finditer(line):
        result.append(escape(line[cursor : token.start()]))
        value = token.group()
        kind = (
            "string"
            if value[0] in "\"'"
            else "key"
            if line[token.end() :].startswith(":")
            else "literal"
        )
        result.append(f'<span class="yaml-{kind}">{escape(value)}</span>')
        cursor = token.end()
    result.append(escape(line[cursor:]))
    return "".join(result)


def describe_benchmark(benchmark):
    """Return the README title and authored dataset type for the gallery."""
    source_url = REPOSITORY + "/blob/main/benchmarks/" + quote(benchmark.name)
    readme_path = benchmark / "README.md"
    readme_text = readme_path.read_text(encoding="utf-8")
    title, _ = render_readme(readme_text, source_url)
    metadata = load_metadata(benchmark)
    return title, metadata["domain"]


def page_link(name, label, relation):
    if name is None:
        return f'<span class="is-disabled" aria-disabled="true">{label}</span>'
    return f'<a href="{escape(name)}.html" rel="{relation}">{label}</a>'


def render_review_form(name, questions, comment_hint):
    """The survey: one fieldset per question, then a free-text box. The script
    reads the labels, prompts and choices back from these elements to write
    the discussion body, so each question is declared once."""
    discussions_url = (
        REPOSITORY + "/discussions?discussions_q=" + quote(f'"{name}" in:title')
    )
    next_url = f"{REVIEW_QUEUE_URL}?after={quote(name)}"
    fieldsets = []
    for key, target, label, prompt, choices in questions:
        options = "".join(
            '<label class="review-choice"><input type="radio" '
            f'name="{key}" value="{escape(choice)}"> {escape(choice)}</label>'
            for choice in choices
        )
        fieldsets.append(
            f'<fieldset class="review-question" data-label="{escape(label)}">'
            f'<legend><a href="{escape(target)}"><strong>{escape(label)}:</strong></a> '
            f'<span class="review-prompt">{escape(prompt)}</span></legend>'
            f'<div class="review-choices">{options}</div></fieldset>'
        )
    return (
        '<div class="material-toolbar"><h3 class="material-title">Your review</h3>'
        '<p class="material-provenance">Two questions and your comments '
        f'&middot; <a href="{escape(discussions_url)}">View benchmark discussions</a></p>'
        "</div>"
        f'<form class="review-form" data-benchmark="{escape(name)}" '
        f'data-review-url="{escape(REVIEW_URL)}" '
        f'data-next-url="{escape(next_url)}" '
        f'data-approve="{escape(AGREEMENT[0])}" '
        f'data-done-key="{escape(REVIEW_DONE_KEY)}" novalidate hidden>'
        + "".join(fieldsets)
        + '<label class="review-comments-label" for="review-comments">'
        "Comments <small>Required if you disagree</small></label>"
        '<textarea id="review-comments" name="comments" rows="4" '
        f'maxlength="10000" placeholder="{escape(comment_hint)}"></textarea>'
        '<p class="review-feedback" role="status" aria-live="polite" hidden></p>'
        '<div class="review-actions"><p class="review-note">Drafts save '
        "automatically in this browser. Submitting opens GitHub with your review "
        "filled in and moves this page to the next benchmark; select "
        "<strong>Start discussion</strong> on GitHub to publish your review "
        "under your account. Skip any benchmark you cannot judge.</p>"
        '<div class="review-buttons"><button type="button" class="review-skip">'
        'Skip</button><button type="submit" class="review-submit">'
        "Submit and next</button></div></div>"
        '<div class="review-prepared" hidden><label for="review-prepared-text">'
        'Review text to copy</label><textarea id="review-prepared-text" rows="8" '
        f'readonly></textarea><a href="{escape(next_url)}">Next benchmark</a>'
        "</div></form>"
        "<noscript><p>The review form needs JavaScript. Leave your review in "
        f'<a href="{escape(REPOSITORY)}/discussions">GitHub Discussions</a>.</p>'
        "</noscript>"
    )


def plural(count, noun):
    return f"{count} {noun}" + ("" if count == 1 else "s")


def benchmark_group(name):
    family = name.partition("-")[0]
    family = "sdtm" if family == "send" else family
    if family not in GROUP_NOTES:
        raise ValueError(
            f"benchmark family has no gallery description: {family}-* "
            f"(add it to GROUP_NOTES in generate.py)"
        )
    return family


def render_groups(names):
    """Count the same gallery groups used by the listing and jump links."""
    counts = {}
    for name in names:
        family = benchmark_group(name)
        counts[family] = counts.get(family, 0) + 1
    rows = "".join(
        f"| `{family}` | {counts[family]} | {escape(GROUP_NOTES[family])} |\n"
        for family in GROUP_NOTES
        if family in counts
    )
    return "| Group | Count | Purpose |\n|---|---|---|\n" + rows.rstrip("\n")


def curated_links(template_text):
    """Return the benchmark names the overview hand-picks.

    The overview names a reading path. Those links outlive the directories
    they point at unless something checks them, so `main` compares them
    against the suite and a rename breaks the build instead of shipping a
    dead link.
    """
    return sorted(set(re.findall(r"\]\(([a-z0-9-]+)\.html\)", template_text)))


def gallery_families(entries):
    """Sort the entries into the gallery's groups, then its dataset types."""
    families = {key: {} for key in GROUP_NOTES}
    for name, title, category in entries:
        families[benchmark_group(name)].setdefault(category, []).append((name, title))
    return families


def review_queue(entries):
    """The benchmarks under review, in the order the gallery lists them."""
    families = gallery_families(entries)
    return [
        name
        for key in REVIEW_GROUPS
        for category in sorted(families[key])
        for name, _ in sorted(families[key][category])
    ]


def render_review_queue(entries, saved):
    """The page that sends a reviewer to their next benchmark: first those that
    need one more review to settle, then those nobody has reviewed."""
    reviews = saved["reviews"] if saved else []
    counted = review_status.tally(review_queue(entries), reviews)
    queue = {
        "doneKey": REVIEW_DONE_KEY,
        "started": [
            name
            for name, (count, _) in counted.items()
            if 0 < count < review_status.REQUIRED_REVIEWS
        ],
        "open": [name for name, (count, _) in counted.items() if not count],
    }
    template = Template((HERE / "review-queue.html").read_text(encoding="utf-8"))
    result = template.substitute(
        queue=json.dumps(queue),
        script=(HERE / "review-queue.js").read_text(encoding="utf-8"),
    )
    return result.encode("ascii", "xmlcharrefreplace")


def render_review_status(entries, saved):
    """The Status section of the review page, from the saved reviews. Votes stay
    out of it: a benchmark shows how many people reviewed it until it settles."""
    if saved is None:
        return "Status is not available in this build."
    counted = review_status.tally(review_queue(entries), saved["reviews"])
    categories = {name: category for name, _, category in entries}
    columns = review_status.STATUSES
    groups = [(f"`{key}`", key) for key in REVIEW_GROUPS] + [("**All**", None)]
    totals = []
    for label, key in groups:
        found = [
            found
            for name, (_, found) in counted.items()
            if key in (None, benchmark_group(name))
        ]
        cells = [str(found.count(column)) for column in columns]
        totals.append(f"| {label} | {len(found)} | " + " | ".join(cells) + " |\n")
    rows = [
        f'| <a href="../{escape(name)}.html">{escape(name)}</a> '
        f"| {escape(categories[name])} | {reviews} | {found} |\n"
        for name, (reviews, found) in counted.items()
    ]
    return (
        f"Updated {escape(saved['updated'])}.\n\n"
        "| Group | Benchmarks | " + " | ".join(columns) + " |\n"
        "|---|---|---|---|---|---|\n" + "".join(totals) + "\n"
        f"| Benchmark | Domain | Reviews ({review_status.REQUIRED_REVIEWS} needed) "
        "| Status |\n|---|---|---|---|\n" + "".join(rows)
    )


def render_review_page(entries, saved):
    """The page that says what the review is for, and how far it has come."""
    names = review_queue(entries)
    reviews = saved["reviews"] if saved else []
    logins = sorted(
        {item["reviewer"] for item in reviews if item["benchmark"] in names}
    )
    template = Template(REVIEW_PAGE.read_text(encoding="utf-8"))
    result = template.substitute(
        total=plural(len(names), "benchmark"),
        reviewers=", ".join(f"`{escape(login)}`" for login in logins) or "None yet",
        status=render_review_status(entries, saved),
    )
    return result.encode("ascii", "xmlcharrefreplace")


def render_index(entries):
    """Render the gallery page linking every generated dashboard.

    The page is Markdown, so MkDocs renders it inside the documentation site
    with the same header, navigation, search, and palette as every other page;
    only the dashboards themselves stay self-contained. Headings are Markdown
    so they reach the table of contents, and the lists are written as HTML so
    a title is escaped exactly once and the stylesheet can lay them out.

    The overview table, jump links and listing share the same group order.
    Each subsection holds one dataset type from the benchmark metadata,
    including schema examples and specifications that must be rejected.

    The overview above the listing is authored at `docs/articles/benchmark.md`
    and reaches the reader on the same page as the benchmarks it describes.
    Its totals are substituted from the directories rather than typed, and its
    curated links are checked against the suite, so neither can go stale.
    """
    families = gallery_families(entries)
    blocks, jumps = [], []
    for key in GROUP_NOTES:
        groups = families[key]
        if not groups:
            continue
        total = sum(len(items) for items in groups.values())
        jumps.append(f'<a href="#{key}">{key} ({total})</a>')
        note = (
            "Each must fail, and expected/error.yaml pins the error it raises."
            if key == "negative"
            else "Each runs to completion and produces the artifact in expected/."
        )
        sections = [
            f"## {key} {{: #{key} }}\n\n"
            f'<p class="benchmark-note">{plural(total, "benchmark")}. '
            f"{escape(note)}</p>"
        ]
        for category in sorted(groups):
            items = "".join(
                f'<li><a href="{escape(name)}.html">{escape(title)}</a></li>\n'
                for name, title in sorted(groups[category])
            )
            sections.append(
                f"### {escape(category)}\n\n"
                f'<p class="benchmark-count">'
                f"{plural(len(groups[category]), 'benchmark')}</p>\n\n"
                f'<ul class="benchmark-grid">\n{items}</ul>'
            )
        blocks.append("\n\n".join(sections))
    names = [name for name, _, _ in entries]
    template = Template(OVERVIEW.read_text(encoding="utf-8"))
    result = template.substitute(
        total=plural(len(entries), "benchmark"),
        source_url=REPOSITORY + "/tree/main/benchmarks",
        groups=render_groups(names),
        summary="".join(jumps),
        sections="\n\n".join(blocks) + "\n",
    )
    return result.encode("ascii", "xmlcharrefreplace")


def render_spec_pane(filename, text, slug, single, edit_url=None):
    lines = text.splitlines()
    code_lines = []
    for number, line in enumerate(lines, 1):
        line_id = f"yaml-line-{number}" if single else f"{slug}-line-{number}"
        code_lines.append(
            f'<span class="code-line" id="{line_id}"><span class="line-number" aria-hidden="true">{number}</span>'
            f'<span class="code-source">{highlight_yaml(line)}</span></span>'
        )
    if single:
        return "".join(code_lines), len(lines)
    edit = f'<a class="edit-button" href="{edit_url}">Edit</a>' if edit_url else ""
    edit_data = f' data-edit-url="{escape(edit_url)}"' if edit_url else ""
    pane = (
        f'<div class="spec-pane" id="pane-{slug}" data-filename="{escape(filename)}" '
        f'data-lines="{len(lines)}"{edit_data}>'
        f'<div class="file-heading"><span class="file-title"><h3 class="filename">{escape(filename)}</h3>{edit}</span>'
        f'<span class="file-count">{len(lines)} lines</span></div>'
        f"<pre><code>{''.join(code_lines)}</code></pre></div>"
    )
    return pane, len(lines)


def benchmark_code(benchmark):
    """Render public calls from the same ordered metadata as conformance.

    Every call before the last saves its artifact, so a later entry reads it
    at the path its `schema` link types; the last call shows its output.
    """
    if benchmark.name.startswith("negative-"):
        return []
    entries = benchmark_entries(benchmark)
    result = []
    for host, title, opening, spec, project, member in (
        ("r", "R", "library(yamaa)", "yamaa_spec", "yamaa_project", "$"),
        ("python", "Python", "import yamaa", "yamaa.spec", "yamaa.project", "."),
    ):
        calls = []
        for index, entry in enumerate(entries[host], 1):
            if entry["environment"] is None:
                opened = f"{spec}({json.dumps(entry['specification'])})"
            else:
                # A project opens its listed specification by its domain name.
                name = specification_name(benchmark / entry["specification"])
                opened = (
                    f"{project}({json.dumps(entry['environment'])})"
                    f"{member}spec({json.dumps(name)})"
                )
            final = "output" if index == len(entries[host]) else "save()"
            calls.append(f"{opened}{member}run(){member}{final}")
        result.append((title, opening + "\n\n" + "\n".join(calls) + "\n"))
    return result


def slugify(text):
    """An HTML id fragment: lower case, other characters run together as one hyphen."""
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def render_file_document(document_id, title, line_count, body, edit_url=None):
    """One file in a material pane: its name, Edit link, and line count over its body."""
    edit = f'<a class="edit-button" href="{edit_url}">Edit</a>' if edit_url else ""
    return (
        f'<div class="material-document material-file" id="{document_id}" data-filename="{escape(title)}">'
        f'<div class="file-heading"><span class="file-title"><h4 class="filename">{escape(title)}</h4>{edit}</span>'
        f'<span class="file-count">{plural(line_count, "line")}</span></div>{body}</div>'
    )


def render_code_document(path, slug, title, edit_url=None):
    """A code file with numbered lines, each one linkable as #code-<slug>-line-<n>."""
    return render_code_text(path.read_text(encoding="utf-8"), slug, title, edit_url)


def render_code_text(text, slug, title, edit_url=None):
    lines = text.splitlines()
    code_lines = "".join(
        f'<span class="code-line" id="code-{slug}-line-{number}"><span class="line-number" aria-hidden="true">{number}</span>'
        f'<span class="code-source">{escape(line)}</span></span>'
        for number, line in enumerate(lines, 1)
    )
    body = (
        f'<div class="code-scroll" tabindex="0" aria-label="{escape(title)}">'
        f"<pre><code>{code_lines}</code></pre></div>"
    )
    return render_file_document(f"code-pane-{slug}", title, len(lines), body, edit_url)


def render_prompt_document(path, tier, title, edit_url, source_url, text=None):
    """A prompt reads as the request it is; the Edit link opens the raw file.

    It renders under the README's rules: no raw HTML, links resolved against
    the prompt's folder, and images shown as links.
    """
    text = path.read_text(encoding="utf-8") if text is None else text
    markdown = MarkdownIt("commonmark", {"html": False}).enable("table")
    body = render_markdown_tokens(markdown, markdown.parse(text), source_url)
    return render_file_document(
        f"prompt-pane-{tier}",
        title,
        len(text.splitlines()),
        f'<div class="prose prompt-prose">{body}</div>',
        edit_url,
    )


def render_material_pane(key, title, provenance, picker_label, documents):
    """One tab's pane: a toolbar naming where it comes from over its documents.

    `documents` is a list of (picker label, document id, document HTML); a
    picker appears when there is more than one to choose from.
    """
    picker = ""
    if len(documents) > 1:
        options = "".join(
            f'<option value="{document_id}">{escape(label)}</option>'
            for label, document_id, _ in documents
        )
        picker = (
            f'<label class="material-picker"><span>{escape(picker_label)}</span>'
            f'<select id="{key}-select" data-documents>{options}</select></label>'
        )
    return (
        f'<div role="tabpanel" id="{key}" class="material-pane" aria-labelledby="tab-{key}">'
        f'<div class="material-toolbar"><h3 class="material-title">{escape(title)}</h3>{picker}'
        f'<p class="material-provenance">{provenance}</p></div>'
        + "".join(document for _, _, document in documents)
        + "</div>"
    )


def project_codelists(benchmark):
    """The codelists a benchmark's root project file declares, or None.

    A codelist source is inline or a path relative to the project file.
    """
    candidate = benchmark / "project.yaml"
    if not candidate.is_file():
        return None
    project = yaml.safe_load(candidate.read_text(encoding="utf-8"))
    sources = project.get("codelists") if isinstance(project, dict) else None
    codelists = []
    for source in sources if isinstance(sources, list) else []:
        if isinstance(source, str):
            path = candidate.parent / source
            source = (
                yaml.safe_load(path.read_text(encoding="utf-8"))
                if path.is_file()
                else None
            )
        if isinstance(source, dict) and isinstance(source.get("codelists"), list):
            codelists.extend(c for c in source["codelists"] if isinstance(c, dict))
    return {"codelists": codelists} if codelists else None


def render_materials(benchmark, spec, document, code_snippets, edit_base):
    """Tabs and panes for the assessment materials shown beside Comments.

    None of them is under review: the mapping spec is generated from the YAML,
    the yamaa code only runs it, and the prompts and the solutions written
    without yamaa exist to run and score agent evaluations. A material a
    benchmark lacks has no tab.
    """
    materials = []
    sheets = mapping_doc.render_mapping_sheets(spec, document)
    if sheets:
        materials.append(
            (
                "mapping-spec",
                "Mapping spec",
                "Generated from the benchmark YAML; do not edit by hand",
                "Sheet",
                [
                    (
                        label,
                        f"mapping-sheet-{sheet_id}",
                        f'<div class="material-document" id="mapping-sheet-{sheet_id}">{table}</div>',
                    )
                    for sheet_id, label, table in sheets
                ],
            )
        )
    name = quote(benchmark.name)
    prompts = HARBOR / "prompts" / benchmark.name
    tiers = [
        tier
        for tier in PROMPT_TIERS
        if (prompts / ("full.md" if tier[0] == "brief" else f"{tier[0]}.md")).is_file()
    ]
    if tiers:
        materials.append(
            (
                "prompt",
                "Agent prompt",
                (
                    "The request an agent receives with the input files; each "
                    f'<a href="{REPOSITORY}/blob/main/evaluations/harbor/prompts/README.md">tier</a> '
                    "states no more than the one above"
                ),
                "Tier",
                [
                    (
                        f"{label} \u2014 {keeps}",
                        f"prompt-pane-{tier}",
                        render_prompt_document(
                            prompts / ("full.md" if tier == "brief" else f"{tier}.md"),
                            tier,
                            f"prompts/{benchmark.name}/{tier}.md",
                            f"{REPOSITORY}/edit/main/evaluations/harbor/prompts/{name}/{'full' if tier == 'brief' else tier}.md",
                            f"{REPOSITORY}/blob/main/evaluations/harbor/prompts/{name}",
                            text=brief(
                                (prompts / "full.md").read_text(encoding="utf-8")
                            )
                            if tier == "brief"
                            else None,
                        ),
                    )
                    for tier, label, keeps in tiers
                ],
            )
        )
    if code_snippets:
        materials.append(
            (
                "code",
                "yamaa code",
                "Public calls generated from benchmark.yaml; results must match the expected output",
                "File",
                [
                    (
                        title,
                        f"code-pane-{slugify(title)}",
                        render_code_text(
                            text,
                            slugify(title),
                            title,
                            f"{edit_base}/benchmark.yaml" if edit_base else None,
                        ),
                    )
                    for title, text in code_snippets
                ],
            )
        )
    solutions = HARBOR / "solutions" / benchmark.name
    solution_files = [
        solutions / filename
        for filename in SOLUTION_FILES
        if (solutions / filename).is_file()
    ]
    if solution_files:
        materials.append(
            (
                "solution",
                "Solution without yamaa",
                (
                    "Written from the full prompt and the inputs alone; CI checks it "
                    "reproduces the expected output"
                ),
                "File",
                [
                    (
                        path.name,
                        f"code-pane-solution-{slugify(path.name)}",
                        render_code_document(
                            path,
                            "solution-" + slugify(path.name),
                            f"solutions/{benchmark.name}/{path.name}",
                            f"{REPOSITORY}/edit/main/evaluations/harbor/solutions/{name}/{quote(path.name)}",
                        ),
                    )
                    for path in solution_files
                ],
            )
        )
    tabs = "".join(
        f'<button type="button" role="tab" id="tab-{key}" aria-controls="{key}" '
        'aria-selected="false" aria-describedby="not-under-review" '
        f'class="material-tab reference-tab">{escape(title)}</button>'
        for key, title, _, _, _ in materials
    )
    panes = "".join(render_material_pane(*material) for material in materials)
    return tabs, panes


def render_benchmark(benchmark, previous=None, next=None):
    """One benchmark's self-contained dashboard page, as ASCII bytes."""
    source_url = REPOSITORY + "/blob/main/benchmarks/" + quote(benchmark.name)
    edit_base = REPOSITORY + "/edit/main/benchmarks/" + quote(benchmark.name)
    readme_edit_url = edit_base + "/README.md"
    readme_path = benchmark / "README.md"
    spec_path, chain = benchmark_entry(benchmark)
    if spec_path is None:
        raise ValueError(f"benchmark has no spec file: {benchmark.name}")
    spec_edit_url = edit_base + "/" + quote(spec_path.name)
    readme_text = readme_path.read_text(encoding="utf-8")
    metadata = load_metadata(benchmark)
    lifecycle_state = metadata["lifecycle"]
    lifecycle_url = f"https://img.shields.io/badge/Lifecycle-{lifecycle_state}-{LIFECYCLES[lifecycle_state]}"
    lifecycle_href = LIFECYCLE_URL
    lifecycle_badge = (
        f'<a class="lifecycle-badge" href="{escape(lifecycle_href)}">'
        f'<img src="{escape(lifecycle_url)}" alt="Lifecycle: {escape(lifecycle_state)}"></a>'
    )
    title, readme = render_readme(readme_text, source_url)
    spec_text = spec_path.read_text(encoding="utf-8")
    # Parse metadata for labels and visual emphasis only; this does not execute the spec.
    spec = yaml.safe_load(spec_text)
    spec = spec if isinstance(spec, dict) else {}
    # A producer kept beside the entry publishes an expected artifact of its
    # own, so its columns label that artifact the way the entry's label theirs.
    column_specs = [spec]
    for producer in sibling_producers(benchmark, spec):
        document = yaml.safe_load(producer.read_text(encoding="utf-8"))
        column_specs.append(document if isinstance(document, dict) else {})
    labels = {}
    derived = set()
    for owner in reversed(column_specs):
        declared = owner.get("columns", [])
        if not isinstance(declared, list):
            declared = []
        columns = [item for item in declared if isinstance(item, dict)]
        labels.update(
            (item["name"], item["label"])
            for item in columns
            if "name" in item and "label" in item
        )
        for item in columns:
            name = item.get("name")
            expression = item.get("derivation", {})
            expression = expression if isinstance(expression, dict) else {}
            source = expression.get("source")
            if isinstance(source, dict):
                source = source.get("variable")
            if name and source != f"{owner.get('base')}.{name}":
                derived.add(name)
    inputs = fixture_files(benchmark / "input")
    error_path = benchmark / "expected" / "error.yaml"
    is_failure = error_path.is_file()
    outputs = [
        path
        for path in fixture_files(benchmark / "expected")
        if path.name != SPEC_RESOLVED_NAME
        and (not is_failure or path.name != "error.yaml")
    ]
    input_files, _, input_subjects = render_files(inputs, "input", benchmark, set(), {})
    output_files, output_rows, output_subjects = render_files(
        outputs, "output", benchmark, derived, labels
    )
    subjects = sorted(input_subjects | output_subjects)
    subject_ids = [json.loads(key)[1] for key in subjects]
    subject_options = []
    for key in subjects:
        study, subject = json.loads(key)
        label = f"{study} / {subject}" if subject_ids.count(subject) > 1 else subject
        subject_options.append(
            f'<option value="{escape(key)}">{escape(label)}</option>'
        )
    category = ".".join((metadata["standard"], metadata["domain"]))
    heading = title.split(": ", 1)[-1]
    heading = heading[:1].upper() + heading[1:]
    has_csv = any(path.suffix == ".csv" for path in outputs)
    if is_failure:
        failure, failure_section = render_failure_section(
            error_path, edit_base + "/expected/error.yaml"
        )
        datasets_heading = "Unexpected Output"
        description = escape(
            title + ": README, inputs, expected failure, and YAML specification."
        )
        metrics = [
            ("Rejected", "result", "result-rejected"),
            (len(inputs), "input files", ""),
            (len(subjects), "subjects", ""),
        ]
        if has_csv:
            metrics.append((output_rows, "rows presented", ""))
            output_caption = (
                f"{output_rows} row presented to the failing check"
                if output_rows == 1
                else f"{output_rows} rows presented to the failing check"
            ) + " - not an accepted artifact"
        else:
            output_caption = "No artifact is produced"
    else:
        failure_section = ""
        datasets_heading = "Expected output"
        description = escape(
            title + ": README, inputs, expected output, and YAML specification."
        )
        metrics = [(len(inputs), "input files", ""), (len(subjects), "subjects", "")]
        if has_csv:
            metrics.append((output_rows, "expected rows", ""))
            output_caption = (
                f"{output_rows} expected row"
                if output_rows == 1
                else f"{output_rows} expected rows"
            )
        else:
            metrics.append((len(outputs), "expected files", ""))
            output_caption = "Expected artifacts"

    # What reviewers are asked to challenge, the task included: a short survey
    # on the use case and the data structures, where agreeing with both approves
    # the benchmark. Everything in the tabs beside Your review is background.
    if is_failure:
        questions = (
            (
                "task",
                "#readme",
                "Task",
                (
                    "Is this a specification error a study team could realistically "
                    "make, and should the build stop on it rather than continue?"
                ),
                AGREEMENT,
            ),
            (
                "inputs",
                "#inputs",
                "Inputs",
                "Do the inputs trigger the failure for the reason the summary gives?",
                AGREEMENT,
            ),
        )
    else:
        questions = (
            (
                "task",
                "#readme",
                "Task",
                (
                    "Would you agree that the benchmark use case represents real "
                    "data derivation in clinical trials?"
                ),
                AGREEMENT,
            ),
            (
                "inputs",
                "#inputs",
                "Inputs",
                (
                    "Would you agree that the inputs and outputs represent real "
                    "data structures collected in clinical trials?"
                ),
                AGREEMENT,
            ),
        )
    data_scope = "inputs" if is_failure else "inputs and outputs"
    review_ask = (
        "<p>As a clinical trial statistician or programmer, answer two short "
        'questions in <a href="#your-review">Your review</a>: the task and the '
        f"{data_scope}. Agreeing with both approves this benchmark; disagreeing "
        "with either needs a comment. Propose a fix with the "
        "<strong>Edit</strong> button on any source file, or add it to your "
        'comments.</p><p class="review-ask-note">The other tabs beside Your '
        "review are background for assessment, not under review.</p>"
    )
    review_form = render_review_form(
        benchmark.name, questions, "What should change to finalize the benchmark"
    )

    def metric_cell(count, label, cls):
        klass = f' class="{cls}"' if cls else ""
        return f"<div{klass}><dt>{label}</dt><dd>{count}</dd></div>"

    metrics_html = "".join(metric_cell(*item) for item in metrics)
    resolved_path = benchmark / "expected" / SPEC_RESOLVED_NAME
    if not chain and not resolved_path.is_file():
        spec_code, spec_line_count = render_spec_pane(
            spec_path.name, spec_text, "yaml", True
        )
        spec_caption = "1 spec file"
        spec_file_header = (
            '<div class="file-heading spec-file-heading"><span class="file-title">'
            f'<h3 class="filename">{escape(spec_path.name)}</h3>'
            f'<a class="edit-button" href="{spec_edit_url}">Edit</a></span>'
            f'<span class="file-count">{spec_line_count} lines</span></div>'
        )
    else:
        panes = []
        sources = list(chain) + [spec_path]
        if resolved_path.is_file():
            sources.append(resolved_path)
        for path in sources:
            try:
                relpath = path.relative_to(benchmark).as_posix()
            except ValueError:
                relpath = None
            text = path.read_text(encoding="utf-8")
            filename = path.name
            file_edit = (
                edit_base + "/" + "/".join(quote(part) for part in relpath.split("/"))
                if relpath
                else None
            )
            pane, count = render_spec_pane(
                filename, text, Path(filename).stem, False, file_edit
            )
            panes.append(pane)
            spec_line_count = count
        spec_caption = f"{len(sources)} spec files"
        panes.append(
            f"<script>{(HERE / 'spec-panes.js').read_text(encoding='utf-8')}</script>"
        )
        spec_code = "".join(panes)
        spec_file_header = ""
    code_snippets = benchmark_code(benchmark)
    material_tabs, material_panes = render_materials(
        benchmark, spec, project_codelists(benchmark), code_snippets, edit_base
    )
    template = Template((HERE / "dashboard.html").read_text(encoding="utf-8"))
    result = template.substitute(
        benchmark_name=escape(benchmark.name),
        page_title=escape(title),
        heading=escape(heading),
        category=escape(category),
        description=description,
        lifecycle_badge=lifecycle_badge,
        conformance=escape(
            f"Public conformance excluded: {metadata['conformance']['excluded']}"
            if metadata.get("conformance", {}).get("excluded")
            else "Public entries: Python and R. Passing coverage requires an installed inventory."
        ),
        failure_section=failure_section,
        datasets_heading=datasets_heading,
        readme_edit_url=readme_edit_url,
        spec_file_header=spec_file_header,
        source_url=REPOSITORY + "/tree/main/benchmarks/" + quote(benchmark.name),
        prev_link=page_link(previous, "Previous benchmark", "prev"),
        next_link=page_link(next, "Next benchmark", "next"),
        metrics=metrics_html,
        subject_options="".join(subject_options),
        readme=readme,
        prose_class="prose prose-short"
        if readme_body_line_count(readme_text) <= 10
        else "prose",
        input_files=input_files,
        input_caption=f"{len(inputs)} source file" + ("" if len(inputs) == 1 else "s"),
        output_files=output_files,
        output_caption=output_caption,
        spec_caption=spec_caption,
        spec_code=spec_code,
        review_ask=review_ask,
        review_form=review_form,
        material_tabs=material_tabs,
        material_panes=material_panes,
        panel_script=(HERE / "panel-tabs.js").read_text(encoding="utf-8"),
        styles=(HERE / "dashboard.css").read_text(encoding="utf-8"),
        script=(HERE / "dashboard.js").read_text(encoding="utf-8"),
        review_script=(HERE / "review-form.js").read_text(encoding="utf-8"),
    )
    # Stable bytes across machines, locales, time zones, and checkout locations.
    # Character references also keep generated pages compatible with repository ASCII lint.
    return result.encode("ascii", "xmlcharrefreplace")


def saved_reviews(output_dir):
    """The reviews saved beside the page, or None when a build has none."""
    path = output_dir / "review" / review_status.DESTINATION.name
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "benchmarks",
        nargs="*",
        help="Benchmark directory names; defaults to existing generated dashboards",
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="Generate every benchmark containing README.md and a spec file",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail if a selected dashboard is missing or differs; write nothing",
    )
    parser.add_argument(
        "--quiet", action="store_true", help="Suppress successful generation messages"
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DESTINATION,
        help="Destination directory (default: docs/benchmark)",
    )
    args = parser.parse_args(argv)
    if args.all and args.benchmarks:
        parser.error("choose --all or explicit benchmark names")
    inventory = plan(BENCHMARKS)
    complete = [entry["name"] for entry in inventory["examples"]]
    missing_readmes = [
        name for name in complete if not (BENCHMARKS / name / "README.md").is_file()
    ]
    if missing_readmes:
        parser.error(
            "authored benchmarks missing README.md: " + ", ".join(missing_readmes)
        )
    names = args.benchmarks
    if args.all:
        names = complete
    elif not names:
        names = [path.stem for path in sorted(args.output_dir.glob("*.html"))]
    if not names:
        parser.error("specify a benchmark name or --all")
    ordered = sorted(set(names))
    unexpected = sorted(set(ordered) - set(complete))
    if unexpected:
        parser.error("unexpected benchmark names: " + ", ".join(unexpected))
    overview_text = OVERVIEW.read_text(encoding="utf-8")
    dangling = [
        target for target in curated_links(overview_text) if target not in set(complete)
    ]
    if dangling:
        parser.error(
            "docs/articles/benchmark.md links to benchmarks that do not exist: "
            + ", ".join(dangling)
        )
    neighbors = {
        name: (
            complete[index - 1] if index else None,
            complete[index + 1] if index + 1 < len(complete) else None,
        )
        for index, name in enumerate(complete)
        if name in set(ordered)
    }
    write_index = args.all or (
        not args.benchmarks and (args.output_dir / "index.md").is_file()
    )
    failures = []
    entries = []
    for name in ordered:
        if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name):
            parser.error(f"invalid benchmark name: {name}")
        benchmark = BENCHMARKS / name
        if not (benchmark / "README.md").is_file() or not benchmark_has_spec(benchmark):
            parser.error(f"benchmark must contain README.md and a spec file: {name}")
        try:
            previous, following = neighbors.get(name, (None, None))
            rendered = render_benchmark(benchmark, previous, following)
        except (OSError, ValueError, yaml.YAMLError) as error:
            print(f"Cannot generate {name}: {error}", file=sys.stderr)
            failures.append(name)
            continue
        entries.append((name,) + describe_benchmark(benchmark))
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
        saved = saved_reviews(args.output_dir)
        pages = (
            ("index", args.output_dir / "index.md", render_index(entries)),
            (
                "review page",
                args.output_dir / "review" / "index.md",
                render_review_page(entries, saved),
            ),
            (
                "review queue",
                args.output_dir / REVIEW_QUEUE_URL / "index.html",
                render_review_queue(entries, saved),
            ),
        )
        for label, destination, page in pages:
            if args.check:
                if not destination.is_file() or destination.read_bytes() != page:
                    failures.append(label)
                    print(f"Stale or missing: {destination}", file=sys.stderr)
                elif not args.quiet:
                    print(f"Current: {label}")
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(page)
                if not args.quiet:
                    print(f"Generated: {destination}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
