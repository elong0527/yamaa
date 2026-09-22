import csv
import importlib.util
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from html.parser import HTMLParser
from pathlib import Path


HERE = Path(__file__).resolve().parent
module_spec = importlib.util.spec_from_file_location("generate", HERE / "generate.py")
generate = importlib.util.module_from_spec(module_spec)
module_spec.loader.exec_module(generate)
BENCHMARK = generate.BENCHMARKS / "adam-adae-death"


class DashboardContent(HTMLParser):
    def __init__(self, page):
        super().__init__(convert_charrefs=True)
        self.cells = []
        self.downloads = []
        self.file_panes = []
        self.tabs = []
        self.code = []
        self.sections = []
        self.code_depth = 0
        self.in_mapping = False
        self.feed(page)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag in {"section", "aside"}:
            self.sections.append(attrs.get("id"))
            if attrs.get("id") == "mapping-spec":
                self.in_mapping = True
        if tag == "td" and not self.in_mapping:
            self.cells.append(attrs["data-value"])
        if tag == "a" and "download" in attrs:
            self.downloads.append(attrs)
        if "file-pane" in attrs.get("class", "").split():
            self.file_panes.append(attrs)
        if attrs.get("role") == "tab":
            self.tabs.append(attrs)
        if tag == "span":
            if self.code_depth:
                self.code_depth += 1
            elif attrs.get("class") == "code-source":
                self.code_depth = 1
                self.code.append("")

    def handle_endtag(self, tag):
        if tag == "span" and self.code_depth:
            self.code_depth -= 1
        if tag == "section" and self.in_mapping:
            self.in_mapping = False

    def handle_data(self, data):
        if self.code_depth:
            self.code[-1] += data


class DashboardTests(unittest.TestCase):
    def test_source_values_and_yaml_are_exact(self):
        content = DashboardContent(generate.render_benchmark(BENCHMARK).decode("ascii"))
        expected_cells = []
        for relative in ["input/ae.csv", "input/dm.csv", "expected/adae.csv"]:
            path = BENCHMARK / relative
            with path.open(newline="") as stream:
                rows = list(csv.reader(stream))
            expected_cells.extend(value for row in rows[1:] for value in row)
        self.assertEqual(content.cells, expected_cells)
        spec_lines = (BENCHMARK / "spec.yaml").read_text().splitlines()
        run_lines = (BENCHMARK / "run.py").read_text().splitlines()
        self.assertEqual(content.code, spec_lines + run_lines)
        self.assertEqual(
            content.sections,
            [
                "readme",
                "specification",
                "inputs",
                "outputs",
                "mapping-spec",
                "code",
                "comments",
            ],
        )
        self.assertEqual(content.downloads, [])
        self.assertEqual(
            [tab["id"] for tab in content.tabs],
            ["mapping-tab-mapping", "mapping-tab-revision-history"],
        )
        self.assertEqual(
            [pane["aria-label"] for pane in content.file_panes],
            ["input/ae.csv", "input/dm.csv", "expected/adae.csv"],
        )
        self.assertTrue(all("hidden" not in pane for pane in content.file_panes))

    def test_readme_taxonomy_moves_above_title_and_short_summary_is_one_column(self):
        benchmark = generate.BENCHMARKS / "adam-advs-bmi"
        page = generate.render_benchmark(benchmark).decode("ascii")
        header, _, summary = page.partition('<section id="readme"')
        self.assertIn('<p class="eyebrow">ADaM.ADVS</p>', header)
        self.assertIn("<h1>Derive BMI</h1>", header)
        self.assertNotIn("text-transform: uppercase", page)
        self.assertIn('<div class="prose prose-short"', summary)
        self.assertNotIn("Standard:", summary.partition("</section>")[0])
        self.assertNotIn("Domain:", summary.partition("</section>")[0])
        self.assertEqual(
            generate.readme_body_line_count(
                benchmark.joinpath("README.md").read_text()
            ),
            7,
        )

    def test_long_summary_keeps_multicolumn_class(self):
        benchmark = generate.BENCHMARKS / "adam-adae-query-flags"
        self.assertGreater(
            generate.readme_body_line_count(
                benchmark.joinpath("README.md").read_text()
            ),
            10,
        )
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertIn('<div class="prose" aria-label="README content">', page)
        self.assertNotIn('<div class="prose prose-short"', page)

    def test_bytes_do_not_depend_on_checkout_location_or_mtime(self):
        original = generate.render_benchmark(BENCHMARK)
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / BENCHMARK.name
            shutil.copytree(BENCHMARK, copied)
            for path in copied.rglob("*"):
                if path.is_file():
                    path.touch()
            self.assertEqual(generate.render_benchmark(copied), original)
        self.assertEqual(generate.render_benchmark(BENCHMARK), original)

    def test_source_edits_change_page_and_check_does_not_write(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            copied = folder / BENCHMARK.name
            shutil.copytree(BENCHMARK, copied)
            (copied / "README.md").write_text(
                (copied / "README.md").read_text() + "\nAdditional explanation.\n"
            )
            self.assertNotEqual(
                generate.render_benchmark(copied), generate.render_benchmark(BENCHMARK)
            )
            output = folder / "output"
            output.mkdir()
            page = output / (BENCHMARK.name + ".html")
            command = [
                sys.executable,
                str(HERE / "generate.py"),
                BENCHMARK.name,
                "--check",
                "--output-dir",
                str(output),
            ]
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
            self.assertFalse(page.exists())
            complete = sorted(
                path.name
                for path in generate.BENCHMARKS.iterdir()
                if generate.benchmark_has_spec(path) and (path / "README.md").is_file()
            )
            index = complete.index(BENCHMARK.name)
            previous = complete[index - 1] if index else None
            following = complete[index + 1] if index + 1 < len(complete) else None
            page.write_bytes(generate.render_benchmark(BENCHMARK, previous, following))
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            page.write_bytes(b"outdated page")
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
            self.assertEqual(page.read_bytes(), b"outdated page")

    def test_markup_and_multiline_csv_values_are_preserved_as_data(self):
        value = '<script>alert("unsafe")</script>\nA & B | C'
        rendered = generate.render_table(["TEXT"], [[value]], "quoted.csv", set(), {})
        self.assertNotIn("<script>", rendered)
        self.assertEqual(DashboardContent(rendered).cells, [value])
        _, readme = generate.render_readme(
            '# Benchmark\n\n<script>alert("unsafe")</script>\n\n[File](input/ae.csv)',
            "https://benchmark.org/fixture",
        )
        self.assertNotIn("<script>", readme)
        self.assertIn('href="https://benchmark.org/fixture/input/ae.csv"', readme)

    def test_readme_taxonomy_is_metadata_not_summary_content(self):
        source = "# Benchmark\n\nBody.\n\n**Standard:** ADaM | **Domain:** ADAE\n"
        self.assertEqual(generate.readme_taxonomy(source), ("ADaM", "ADAE"))
        title, readme = generate.render_readme(source, "https://benchmark.org/fixture")
        self.assertEqual(title, "Benchmark")
        self.assertEqual(readme, "<p>Body.</p>\n")

    def test_negative_fixture_is_shown_without_repair(self):
        benchmark = generate.BENCHMARKS / "negative-source-extra-field"
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertIn("raw CSV", page)
        self.assertIn("No artifact is produced", page)
        self.assertIn(
            generate.escape((benchmark / "expected/error.yaml").read_text()), page
        )

    def test_negative_failure_has_banner_and_own_section(self):
        benchmark = generate.BENCHMARKS / "negative-output-duplicate"
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertIn(
            '<div class="result-rejected"><dt>result</dt><dd>Rejected</dd></div>', page
        )
        self.assertIn('id="expected-failure"', page)
        self.assertIn('<h2 id="outputs-heading">Unexpected Output</h2>', page)
        self.assertIn("not an accepted artifact", page)
        self.assertIn("<div><dt>phase</dt><dd>output</dd></div>", page)
        self.assertIn("<div><dt>requirement</dt><dd>REQ-0240</dd></div>", page)
        self.assertIn("<details><summary>expected/error.yaml</summary>", page)
        base = "https://github.com/elong0527/yamaa/edit/main/benchmarks/negative-output-duplicate"
        self.assertIn(
            '<h2 id="expected-failure-heading">Expected failure</h2>'
            f'<a class="edit-button" href="{base}/expected/error.yaml">Edit</a>',
            page,
        )
        self.assertNotIn("<dt>spec paths</dt>", page)
        self.assertIn("<code>README.md</code>", page)
        self.assertIn(f'<a class="edit-button" href="{base}/README.md">Edit</a>', page)
        self.assertIn(f'<a class="edit-button" href="{base}/spec.yaml">Edit</a>', page)
        self.assertIn(
            f'<a class="edit-button" href="{base}/input/dm.csv">Edit</a>', page
        )
        self.assertIn(
            f'<a class="edit-button" href="{base}/expected/adsl.csv">Edit</a>', page
        )
        # error.yaml leaves the datasets: one pane per CSV, none for the YAML.
        content = DashboardContent(page)
        self.assertIn("expected-failure", content.sections)
        self.assertNotIn('aria-label="expected/error.yaml"', page)
        self.assertEqual(
            [pane["aria-label"] for pane in content.file_panes],
            ["input/dm.csv", "expected/adsl.csv"],
        )
        positive = generate.render_benchmark(
            generate.BENCHMARKS / "sdtm-dm-basic"
        ).decode("ascii")
        self.assertNotIn('class="result-rejected"', positive)
        self.assertNotIn('id="expected-failure"', positive)
        self.assertIn('<h2 id="outputs-heading">Expected output</h2>', positive)

    def test_subject_identity_includes_study(self):
        first = generate.subject_key({"STUDYID": "STUDY-A", "USUBJID": "001"})
        second = generate.subject_key({"STUDYID": "STUDY-B", "USUBJID": "001"})
        self.assertNotEqual(first, second)
        self.assertEqual(generate.subject_key({"STUDYID": "STUDY-A"}), "")

    def test_specification_controls_are_generated(self):
        page = generate.render_benchmark(BENCHMARK).decode("ascii")
        self.assertIn("<span>Hide yamaa spec</span>", page)
        self.assertIn('role="separator" aria-label="Resize specification panel"', page)
        self.assertIn('aria-valuenow="740"', page)
        self.assertIn("const DEFAULT_SPEC_WIDTH = 740", page)
        self.assertIn("setSpecWidth(preferredSpecWidth)", page)
        self.assertNotIn(
            "setSpecWidth(specification.getBoundingClientRect().width)", page
        )
        self.assertIn(
            '<div class="file-heading spec-file-heading"><span class="file-title">'
            '<h3 class="filename">spec.yaml</h3>',
            page,
        )
        self.assertIn('<span class="panel-caption">1 spec file</span>', page)
        self.assertNotIn('id="section-select"', page)
        self.assertNotIn("Jump to section", page)

    def test_comments_map_to_a_stable_discussion_per_benchmark(self):
        page = generate.render_benchmark(BENCHMARK).decode("ascii")
        self.assertEqual(page.count('src="https://giscus.app/client.js"'), 1)
        self.assertIn(
            'data-mapping="specific" data-term="yaml/examples/adam-adae-death" data-strict="1"',
            page,
        )
        self.assertIn('data-repo="elong0527/yamaa"', page)
        self.assertIn(
            'data-theme="https://elong0527.github.io/yamaa/assets/giscus-yamaa.css?v=3"',
            page,
        )
        other = generate.render_benchmark(generate.BENCHMARKS / "sdtm-dm-basic").decode(
            "ascii"
        )
        self.assertIn('data-term="yaml/examples/sdtm-dm-basic"', other)

    def test_giscus_theme_limits_reactions_to_thumbs_with_counts(self):
        theme = (generate.ROOT / "docs/assets/giscus-yamaa.css").read_text(
            encoding="utf-8"
        )
        self.assertIn('aria-label*="+1"', theme)
        self.assertIn('aria-label*="-1"', theme)
        self.assertIn('content: "0"', theme)
        self.assertIn("content-visibility: visible !important", theme)
        self.assertIn("visibility: visible !important", theme)
        self.assertIn("transform: none !important", theme)
        self.assertIn(".gsc-social-reaction-summary-item-count", theme)

    def test_code_panel_lists_benchmark_scripts(self):
        benchmark = generate.BENCHMARKS / "sdtm-dm-basic"
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertIn('id="code"', page)
        self.assertIn('data-filename="run.py"', page)
        self.assertIn("import yamaa", page)
        self.assertIn(
            '<a class="edit-button" href="https://github.com/elong0527/yamaa/edit/main/benchmarks/sdtm-dm-basic/run.py">Edit</a>',
            page,
        )
        self.assertNotIn('id="code-select"', page)
        codeless = generate.BENCHMARKS / "negative-ambiguous-type"
        plain = generate.render_benchmark(codeless).decode("ascii")
        self.assertNotIn('id="code"', plain)

    def test_code_panel_switches_between_files(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            first = folder / "analysis.py"
            first.write_text("print('one')\n", encoding="utf-8")
            second = folder / "figure.R"
            second.write_text("x <- 1\n", encoding="utf-8")
            panel = generate.render_code_panel([first, second])
        self.assertIn('id="code-select"', panel)
        self.assertIn(
            '<option value="code-pane-analysis-py" selected>analysis.py</option>', panel
        )
        self.assertIn('<option value="code-pane-figure-r">figure.R</option>', panel)
        self.assertIn('id="code-pane-figure-r" data-filename="figure.R" hidden>', panel)

    def test_multi_level_spec_renders_panes_with_resolved_default(self):
        benchmark = generate.BENCHMARKS / "schema-inheritance"
        entry, chain = generate.benchmark_entry(benchmark)
        self.assertEqual(entry.name, "spec_study.yaml")
        self.assertEqual(
            [path.name for path in chain],
            ["spec_organization.yaml", "spec_compound.yaml"],
        )
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertEqual(
            re.findall(r'data-filename="([^"]+)"', page),
            [
                "spec_organization.yaml",
                "spec_compound.yaml",
                "spec_study.yaml",
                "spec_resolved.yaml",
                "run.py",
            ],
        )
        self.assertIn("Choose specification document", page)
        self.assertIn('<span class="panel-caption">4 spec files</span>', page)
        base = "https://github.com/elong0527/yamaa/edit/main/benchmarks/schema-inheritance"
        for path in [
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_study.yaml",
            "expected/spec_resolved.yaml",
        ]:
            self.assertIn(f'data-edit-url="{base}/{path}"', page)
        self.assertIn('fileHeading.className = "file-heading spec-file-heading"', page)
        self.assertIn("edit.href = active.dataset.editUrl", page)
        self.assertNotIn('aria-label="expected/spec_resolved.yaml"', page)
        self.assertNotIn('id="section-select"', page)
        base = "https://github.com/elong0527/yamaa/edit/main/benchmarks/schema-inheritance"
        for target in (
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_study.yaml",
        ):
            self.assertIn(
                f'<a class="edit-button" href="{base}/{target}">Edit</a>', page
            )
        self.assertIn(
            f'<a class="edit-button" href="{base}/expected/spec_resolved.yaml">Edit</a>',
            page,
        )
        self.assertIn(
            '<header class="section-header"><h2 id="schema-heading">YAML specification</h2>',
            page,
        )

    def test_schema_prefixed_benchmark_gets_its_own_gallery_category(self):
        benchmark = generate.BENCHMARKS / "schema-inheritance"
        title, category = generate.describe_benchmark(benchmark)
        self.assertEqual(title, "Spec Inheritance")
        self.assertEqual(category, "Specification")

    def test_dashboard_badge_is_not_rendered_on_its_own_page(self):
        page = generate.render_benchmark(generate.BENCHMARKS / "sdtm-dm-basic").decode(
            "ascii"
        )
        self.assertNotIn("badge/Dashboard", page)
        self.assertIn("Basic Demographics", page)

    def test_readme_lifecycle_extracts_state_and_badge_url(self):
        text = (
            "# Benchmark\n\n"
            "[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://benchmark.org/x.html)"
            " [![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)]"
            "(https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)\n"
        )
        self.assertEqual(
            generate.readme_lifecycle(text),
            (
                "finalized",
                "https://img.shields.io/badge/Lifecycle-finalized-brightgreen",
            ),
        )

    def test_readme_lifecycle_defaults_to_draft_without_badge(self):
        self.assertEqual(
            generate.readme_lifecycle("# Benchmark\n\nBody.\n"),
            ("draft", "https://img.shields.io/badge/Lifecycle-draft-lightgrey"),
        )

    def test_readme_lifecycle_defaults_to_draft_for_unknown_state(self):
        text = "[![Lifecycle: archived](https://img.shields.io/badge/Lifecycle-archived-red)](https://benchmark.org)\n"
        self.assertEqual(
            generate.readme_lifecycle(text),
            ("draft", "https://img.shields.io/badge/Lifecycle-draft-lightgrey"),
        )

    def test_dashboard_renders_lifecycle_badge_beside_summary(self):
        finalized = generate.render_benchmark(
            generate.BENCHMARKS / "adam-adsl-age-group"
        ).decode("ascii")
        self.assertIn(
            '<img src="https://img.shields.io/badge/Lifecycle-finalized-brightgreen"',
            finalized,
        )
        self.assertIn('alt="Lifecycle: finalized"', finalized)
        self.assertIn(
            '<h2 id="readme-heading">Summary</h2><a class="lifecycle-badge"',
            finalized,
        )
        self.assertIn(
            '<a class="lifecycle-badge" href="https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle">',
            finalized,
        )
        draft = generate.render_benchmark(
            generate.BENCHMARKS / "adam-adlb-ordered-sum"
        ).decode("ascii")
        self.assertIn(
            '<img src="https://img.shields.io/badge/Lifecycle-draft-lightgrey"', draft
        )
        self.assertIn('alt="Lifecycle: draft"', draft)
        self.assertIn(
            '<h2 id="readme-heading">Summary</h2><a class="lifecycle-badge"',
            draft,
        )
        self.assertIn(
            '<a class="lifecycle-badge" href="https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle">',
            draft,
        )

    def test_unterminated_csv_is_not_silently_repaired(self):
        page = generate.render_benchmark(
            generate.BENCHMARKS / "negative-source-unterminated-quote"
        ).decode("ascii")
        self.assertIn("raw CSV", page)

    def test_neighbor_navigation_links_benchmarks(self):
        page = generate.render_benchmark(BENCHMARK, "aaa-first", "zzz-last").decode(
            "ascii"
        )
        self.assertIn(
            '<a href="aaa-first.html" rel="prev">Previous benchmark</a>', page
        )
        self.assertIn('<a href="zzz-last.html" rel="next">Next benchmark</a>', page)
        self.assertIn('<a href="index.html">All benchmarks</a>', page)
        edges = generate.render_benchmark(BENCHMARK, None, None).decode("ascii")
        self.assertIn(
            '<span class="is-disabled" aria-disabled="true">Previous benchmark</span>',
            edges,
        )
        self.assertIn(
            '<span class="is-disabled" aria-disabled="true">Next benchmark</span>',
            edges,
        )

    def test_gallery_link_is_reachable_without_scrolling(self):
        page = generate.render_benchmark(BENCHMARK).decode("ascii")
        header, _, footer = page.partition("</header>")
        self.assertIn('<a class="gallery-link" href="index.html">', header)
        self.assertIn('<a href="index.html">All benchmarks</a>', footer)

    def test_gallery_index_is_sorted_and_deterministic(self):
        entries = [
            ("adam-zzz-one", "Zulu title", "Zulu"),
            ("adam-aaa-two", "Alpha title", "Alpha"),
            ("adam-mmm-three", "Mike title", "Alpha"),
        ]
        first = generate.render_index(entries)
        self.assertEqual(first, generate.render_index(list(reversed(entries))))
        text = first.decode("ascii")
        self.assertLess(text.index("Alpha"), text.index("Zulu"))
        self.assertLess(
            text.index("adam-aaa-two.html"), text.index("adam-mmm-three.html")
        )
        for name in ("adam-zzz-one", "adam-aaa-two", "adam-mmm-three"):
            self.assertIn(f'href="{name}.html"', text)

    def test_gallery_counts_groups_from_the_directory_names(self):
        # The group table is the figure readers quote, so it is counted from
        # the suite rather than typed into the overview, where it would drift.
        text = generate.render_index(
            [
                ("adam-adsl-one", "ADaM ADSL: derive", "ADaM ADSL"),
                ("adam-adsl-two", "ADaM ADSL: derive more", "ADaM ADSL"),
                ("negative-adsl-three", "ADaM ADSL: reject", "ADaM ADSL"),
            ]
        ).decode("ascii")
        self.assertIn("| Group | Count | Purpose |", text)
        self.assertIn("| `adam-*` | 2 | Assess SDTM to ADaM derivations |", text)
        self.assertIn("| `negative-*` | 1 | Assess yamaa error handling |", text)
        # A group with no benchmarks does not get an empty row.
        self.assertNotIn("`sdtm-*`", text)

    def test_gallery_rejects_a_group_it_cannot_describe(self):
        with self.assertRaises(ValueError) as caught:
            generate.render_groups(["mystery-one"])
        self.assertIn("mystery-*", str(caught.exception))

    def test_overview_links_name_benchmarks_that_exist(self):
        # The reading path is hand-picked, so a rename has to fail the build
        # rather than ship a dead link.
        overview = generate.OVERVIEW.read_text(encoding="utf-8")
        targets = generate.curated_links(overview)
        self.assertGreater(len(targets), 0)
        for name in targets:
            self.assertTrue(
                (generate.BENCHMARKS / name / "README.md").is_file(),
                f"the overview links to a missing benchmark: {name}",
            )

    def test_overview_is_authored_as_a_documentation_page(self):
        # The prose belongs where the other articles live, and MkDocs must not
        # publish it: it carries the placeholders the generator substitutes.
        self.assertEqual(
            generate.OVERVIEW, generate.ROOT / "docs/articles/benchmark.md"
        )
        self.assertTrue(generate.OVERVIEW.is_file())
        excluded = (generate.ROOT / "mkdocs.yml").read_text(encoding="utf-8")
        self.assertIn("articles/benchmark.md", excluded)

    def test_gallery_is_a_markdown_page_for_the_documentation_site(self):
        text = generate.render_index(
            [("adam-adsl-one", "ADaM ADSL: derive a flag", "ADaM ADSL")]
        ).decode("ascii")
        # Front matter and Markdown headings: MkDocs renders this page with the
        # site header, navigation, and palette, so it must not be a whole
        # document of its own.
        self.assertTrue(text.startswith("---\ntitle: Benchmark\n"))
        self.assertIn("\nhide:\n  - actions\n", text)
        self.assertNotIn("<!doctype html>", text)
        self.assertNotIn("<body", text)
        self.assertIn("\n# Benchmark\n", text)
        self.assertIn("\n## Benchmarks {: #positive }\n", text)
        self.assertIn("\n### ADaM ADSL\n", text)
        self.assertIn("1 benchmark, generated from", text)

    def test_gallery_lists_rejected_benchmarks_apart_from_positive_ones(self):
        entries = [
            ("adam-adsl-one", "ADaM ADSL: derive a flag", "ADaM ADSL"),
            ("negative-adsl-two", "ADaM ADSL: reject a flag", "ADaM ADSL"),
        ]
        text = generate.render_index(entries).decode("ascii")
        positive = text.index("{: #positive }")
        negative = text.index("{: #negative }")
        self.assertLess(positive, negative)
        self.assertLess(text.index("adam-adsl-one.html"), negative)
        self.assertLess(negative, text.index("negative-adsl-two.html"))
        # One shared domain heading per group, not one shared between them.
        self.assertEqual(text.count("### ADaM ADSL\n"), 2)
        self.assertIn('<a href="#positive">Benchmarks (1)</a>', text)
        self.assertIn('<a href="#negative">Anti-pattern (1)</a>', text)

    def test_gallery_omits_a_group_with_no_benchmarks(self):
        text = generate.render_index(
            [("adam-adsl-one", "ADaM ADSL: derive", "ADaM ADSL")]
        ).decode("ascii")
        self.assertIn("{: #positive }", text)
        self.assertNotIn("{: #negative }", text)


if __name__ == "__main__":
    unittest.main()
