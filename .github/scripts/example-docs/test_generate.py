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
EXAMPLE = generate.EXAMPLES / "adam-adae-death-outcome"


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
        self.feed(page)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag in {"section", "aside"}:
            self.sections.append(attrs.get("id"))
        if tag == "td":
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

    def handle_data(self, data):
        if self.code_depth:
            self.code[-1] += data


class DashboardTests(unittest.TestCase):
    def test_source_values_and_yaml_are_exact(self):
        content = DashboardContent(generate.render_example(EXAMPLE).decode("ascii"))
        expected_cells = []
        for relative in ["input/ae.csv", "input/dm.csv", "expected/adae.csv"]:
            path = EXAMPLE / relative
            with path.open(newline="") as stream:
                rows = list(csv.reader(stream))
            expected_cells.extend(value for row in rows[1:] for value in row)
        self.assertEqual(content.cells, expected_cells)
        self.assertEqual(content.code, (EXAMPLE / "spec.yaml").read_text().splitlines())
        self.assertEqual(content.sections, ["readme", "specification", "inputs", "outputs"])
        self.assertEqual(content.downloads, [])
        self.assertEqual(content.tabs, [])
        self.assertEqual([pane["aria-label"] for pane in content.file_panes], ["input/ae.csv", "input/dm.csv", "expected/adae.csv"])
        self.assertTrue(all("hidden" not in pane for pane in content.file_panes))

    def test_bytes_do_not_depend_on_checkout_location_or_mtime(self):
        original = generate.render_example(EXAMPLE)
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / EXAMPLE.name
            shutil.copytree(EXAMPLE, copied)
            for path in copied.rglob("*"):
                if path.is_file():
                    path.touch()
            self.assertEqual(generate.render_example(copied), original)
        self.assertEqual(generate.render_example(EXAMPLE), original)

    def test_source_edits_change_page_and_check_does_not_write(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            copied = folder / EXAMPLE.name
            shutil.copytree(EXAMPLE, copied)
            (copied / "README.md").write_text((copied / "README.md").read_text() + "\nAdditional explanation.\n")
            self.assertNotEqual(generate.render_example(copied), generate.render_example(EXAMPLE))
            output = folder / "output"
            output.mkdir()
            page = output / (EXAMPLE.name + ".html")
            command = [sys.executable, str(HERE / "generate.py"), EXAMPLE.name, "--check", "--output-dir", str(output)]
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
            self.assertFalse(page.exists())
            complete = sorted(path.name for path in generate.EXAMPLES.iterdir() if generate.example_has_spec(path) and (path / "README.md").is_file())
            index = complete.index(EXAMPLE.name)
            previous = complete[index - 1] if index else None
            following = complete[index + 1] if index + 1 < len(complete) else None
            page.write_bytes(generate.render_example(EXAMPLE, previous, following))
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            page.write_bytes(b"outdated page")
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
            self.assertEqual(page.read_bytes(), b"outdated page")

    def test_markup_and_multiline_csv_values_are_preserved_as_data(self):
        value = '<script>alert("unsafe")</script>\nA & B | C'
        rendered = generate.render_table(["TEXT"], [[value]], "quoted.csv", set(), {})
        self.assertNotIn("<script>", rendered)
        self.assertEqual(DashboardContent(rendered).cells, [value])
        _, readme = generate.render_readme('# Example\n\n<script>alert("unsafe")</script>\n\n[File](input/ae.csv)', "https://example.org/fixture")
        self.assertNotIn("<script>", readme)
        self.assertIn('href="https://example.org/fixture/input/ae.csv"', readme)

    def test_negative_fixture_is_shown_without_repair(self):
        example = generate.EXAMPLES / "negative-source-record-width"
        page = generate.render_example(example).decode("ascii")
        self.assertIn("raw CSV", page)
        self.assertIn("No artifact is produced", page)
        self.assertIn(generate.escape((example / "expected/error.yaml").read_text()), page)

    def test_negative_failure_has_banner_and_own_section(self):
        example = generate.EXAMPLES / "negative-output-duplicate-subject"
        page = generate.render_example(example).decode("ascii")
        self.assertIn('<div class="result-rejected"><dt>result</dt><dd>Rejected</dd></div>', page)
        self.assertIn('id="expected-failure"', page)
        self.assertIn('<h2 id="outputs-heading">Unexpected Output</h2>', page)
        self.assertIn("not an accepted artifact", page)
        self.assertIn('<div><dt>phase</dt><dd>output</dd></div>', page)
        self.assertIn('<div><dt>requirement</dt><dd>R005-52</dd></div>', page)
        self.assertIn("<details><summary>expected/error.yaml</summary>", page)
        base = "https://github.com/elong0527/yamaa/edit/main/yaml/examples/negative-output-duplicate-subject"
        self.assertIn(
            '<h2 id="expected-failure-heading">Expected failure</h2>'
            f'<a class="edit-button" href="{base}/expected/error.yaml">Edit</a>',
            page,
        )
        self.assertNotIn("<dt>spec paths</dt>", page)
        self.assertIn("<code>README.md</code>", page)
        self.assertIn(f'<a class="edit-button" href="{base}/README.md">Edit</a>', page)
        self.assertIn(f'<a class="edit-button" href="{base}/spec.yaml">Edit</a>', page)
        self.assertIn(f'<a class="edit-button" href="{base}/input/dm.csv">Edit</a>', page)
        self.assertIn(f'<a class="edit-button" href="{base}/expected/adsl.csv">Edit</a>', page)
        # error.yaml leaves the datasets: one pane per CSV, none for the YAML.
        content = DashboardContent(page)
        self.assertIn("expected-failure", content.sections)
        self.assertNotIn('aria-label="expected/error.yaml"', page)
        self.assertEqual(
            [pane["aria-label"] for pane in content.file_panes],
            ["input/dm.csv", "expected/adsl.csv"],
        )
        positive = generate.render_example(generate.EXAMPLES / "sdtm-dm-basic").decode("ascii")
        self.assertNotIn('class="result-rejected"', positive)
        self.assertNotIn('id="expected-failure"', positive)
        self.assertIn('<h2 id="outputs-heading">Expected output</h2>', positive)

    def test_subject_identity_includes_study(self):
        first = generate.subject_key({"STUDYID": "STUDY-A", "USUBJID": "001"})
        second = generate.subject_key({"STUDYID": "STUDY-B", "USUBJID": "001"})
        self.assertNotEqual(first, second)
        self.assertEqual(generate.subject_key({"STUDYID": "STUDY-A"}), "")

    def test_specification_controls_are_generated(self):
        page = generate.render_example(EXAMPLE).decode("ascii")
        self.assertIn('<span>Hide Spec</span>', page)
        self.assertIn('role="separator" aria-label="Resize specification panel"', page)
        self.assertNotIn('id="section-select"', page)
        self.assertNotIn("Jump to section", page)

    def test_code_panel_lists_example_scripts(self):
        example = generate.EXAMPLES / "sdtm-dm-basic"
        page = generate.render_example(example).decode("ascii")
        self.assertIn('id="code"', page)
        self.assertIn('data-filename="run.py"', page)
        self.assertIn("import yamaa", page)
        self.assertNotIn('id="code-select"', page)
        plain = generate.render_example(EXAMPLE).decode("ascii")
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
        self.assertIn('<option value="code-pane-analysis-py" selected>analysis.py</option>', panel)
        self.assertIn('<option value="code-pane-figure-r">figure.R</option>', panel)
        self.assertIn('id="code-pane-figure-r" data-filename="figure.R" hidden>', panel)

    def test_multi_level_spec_renders_panes_with_resolved_default(self):
        example = generate.EXAMPLES / "spec-inheritance"
        entry, chain = generate.example_entry(example)
        self.assertEqual(entry.name, "spec_study.yaml")
        self.assertEqual(
            [path.name for path in chain],
            ["spec_organization.yaml", "spec_compound.yaml"],
        )
        page = generate.render_example(example).decode("ascii")
        self.assertEqual(
            re.findall(r'data-filename="([^"]+)"', page),
            ["spec_organization.yaml", "spec_compound.yaml", "spec_study.yaml", "spec_resolved.yaml"],
        )
        self.assertIn("Choose specification document", page)
        self.assertNotIn('aria-label="expected/spec_resolved.yaml"', page)
        self.assertNotIn('id="section-select"', page)

    def test_spec_prefixed_example_gets_its_own_gallery_category(self):
        example = generate.EXAMPLES / "spec-inheritance"
        title, category = generate.describe_example(example)
        self.assertEqual(title, "Spec Inheritance")
        self.assertEqual(category, "Specification")

    def test_dashboard_badge_is_not_rendered_on_its_own_page(self):
        page = generate.render_example(generate.EXAMPLES / "sdtm-dm-basic").decode("ascii")
        self.assertNotIn("shields.io", page)
        self.assertIn("Create DM from EDC extract", page)

    def test_unterminated_csv_is_not_silently_repaired(self):
        page = generate.render_example(generate.EXAMPLES / "negative-source-unterminated-quote").decode("ascii")
        self.assertIn("raw CSV", page)

    def test_neighbor_navigation_links_examples(self):
        page = generate.render_example(EXAMPLE, "aaa-first", "zzz-last").decode("ascii")
        self.assertIn('<a href="aaa-first.html" rel="prev">Previous example</a>', page)
        self.assertIn('<a href="zzz-last.html" rel="next">Next example</a>', page)
        self.assertIn('<a href="index.html">All examples</a>', page)
        edges = generate.render_example(EXAMPLE, None, None).decode("ascii")
        self.assertIn('<span class="is-disabled" aria-disabled="true">Previous example</span>', edges)
        self.assertIn('<span class="is-disabled" aria-disabled="true">Next example</span>', edges)

    def test_gallery_link_is_reachable_without_scrolling(self):
        page = generate.render_example(EXAMPLE).decode("ascii")
        header, _, footer = page.partition("</header>")
        self.assertIn('<a class="gallery-link" href="index.html">', header)
        self.assertIn('<a href="index.html">All examples</a>', footer)

    def test_gallery_index_is_sorted_and_deterministic(self):
        entries = [
            ("zzz-one", "Zulu title", "Zulu"),
            ("aaa-two", "Alpha title", "Alpha"),
            ("mmm-three", "Mike title", "Alpha"),
        ]
        first = generate.render_index(entries)
        self.assertEqual(first, generate.render_index(list(reversed(entries))))
        text = first.decode("ascii")
        self.assertLess(text.index("Alpha"), text.index("Zulu"))
        self.assertLess(text.index("aaa-two.html"), text.index("mmm-three.html"))
        for name in ("zzz-one", "aaa-two", "mmm-three"):
            self.assertIn(f'href="{name}.html"', text)

    def test_gallery_lists_rejected_examples_apart_from_positive_ones(self):
        entries = [
            ("adam-adsl-one", "ADaM ADSL: derive a flag", "ADaM ADSL"),
            ("negative-adsl-two", "ADaM ADSL: reject a flag", "ADaM ADSL"),
        ]
        text = generate.render_index(entries).decode("ascii")
        self.assertLess(text.index('id="positive"'), text.index('id="negative"'))
        self.assertLess(text.index("adam-adsl-one.html"), text.index('id="negative"'))
        self.assertLess(text.index('id="negative"'), text.index("negative-adsl-two.html"))
        # One shared domain heading per group, not one shared between them.
        self.assertEqual(text.count("<h3>ADaM ADSL</h3>"), 2)
        self.assertIn('<a href="#positive">Examples (1)</a>', text)
        self.assertIn('<a href="#negative">Anti-pattern (1)</a>', text)

    def test_gallery_omits_a_group_with_no_examples(self):
        text = generate.render_index([("adam-adsl-one", "ADaM ADSL: derive", "ADaM ADSL")]).decode("ascii")
        self.assertIn('id="positive"', text)
        self.assertNotIn('id="negative"', text)


if __name__ == "__main__":
    unittest.main()
