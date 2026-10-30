import csv
import functools
import html
import json
import re
import shutil
import tempfile
import unittest
from html.parser import HTMLParser
from pathlib import Path
from unittest import mock
from urllib.parse import unquote

from lxml import html as html_tree

import yaml
from tools.benchmark_docs import generate
from tools.tests import support

HERE = Path(__file__).resolve().parent

BENCHMARK = generate.BENCHMARKS / "adam-adae-death"
SOLUTIONS = generate.HARBOR / "solutions" / BENCHMARK.name


@functools.cache
def benchmark_page():
    """The dashboard of BENCHMARK as the checkout renders it, rendered once for
    the tests that read it."""
    return generate.render_benchmark(BENCHMARK)


# The resolution of the schema-inheritance entry chain. The committed file
# (benchmarks/schema-inheritance/expected/spec_resolved.yaml) was removed at
# the qualified cutover, so the dashboard test stages this content itself in
# a copied benchmark folder.
SCHEMA_INHERITANCE_RESOLVED_SPEC = """\
yamaa: "1.0"
type: spec
domain: ADLB
keys:
  - USUBJID
  - PARAMCD
input:
  LB:
    path: input/lb.csv
    types:
      USUBJID: str
      LBTESTCD: str
      LBSTRESN: float
      LBSTRESU: str
base: LB
output:
  path: adlb.csv
  columns:
    - USUBJID
    - PARAMCD
    - AVAL
    - AVALU
columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation:
      value:
        source:
          variable: LB.USUBJID
  - name: PARAMCD
    type: str
    label: Parameter Code
    derivation:
      value:
        source:
          variable: LB.LBTESTCD
  - name: AVAL
    type: float
    label: Analysis Value
    derivation:
      value:
        source:
          variable: LB.LBSTRESN
    metadata:
      analysis_role: analysis-value
  - name: AVALU
    type: str
    label: Standardized Analysis Unit
    derivation:
      value:
        source:
          variable: LB.LBSTRESU
verifications:
  - unique:
      columns:
        - USUBJID
        - PARAMCD
      severity: error
metadata:
  scope: study
"""


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
        if tag == "table" and "mapping-table" in attrs.get("class", "").split():
            self.in_mapping = True
        if tag == "td" and not self.in_mapping and "data-value" in attrs:
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
        if tag == "table":
            self.in_mapping = False

    def handle_data(self, data):
        if self.code_depth:
            self.code[-1] += data


class DashboardTests(unittest.TestCase):
    def test_transport_files_have_no_preview_panes(self):
        # The display seam omits transport bytes without changing the fixtures.
        benchmark = generate.BENCHMARKS / "send-bw-sendig311"
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertEqual(
            [pane["aria-label"] for pane in DashboardContent(page).file_panes],
            ["input/body_weights.csv", "expected/bw.csv", "expected/define.xml"],
        )
        fixtures = HERE / "fixtures/benchmark_docs"
        cases = json.loads((fixtures / "transport-previews.json").read_text())
        for case in cases:
            with self.subTest(path=case["path"]):
                root = fixtures / case["root"]
                rendered, count, subjects = generate.render_files(
                    [root / case["path"]], "input", root, set(), {}
                )
                self.assertEqual(
                    {
                        "panes": DashboardContent(rendered).file_panes,
                        "row_count": count,
                        "subjects": sorted(subjects),
                        "message": html_tree.fromstring(rendered).text_content(),
                    },
                    case["expected"],
                )

    def test_define_xml_resolves_variables_origins_and_methods(self):
        # Resolved display reads independently authored metadata, not the spec.
        cases = [
            (
                "send-bw-sendig311",
                "BWSTRESN",
                [
                    "BWSTRESN",
                    "Standardized Result in Numeric Format",
                    "float",
                    "8",
                    "1",
                    "No",
                    "Result Qualifier",
                    "",
                    "Derived; Sponsor",
                    "The collected result read as a number.",
                ],
            ),
            (
                "schema-define-2-0",
                "SUBJID",
                [
                    "SUBJID",
                    "Subject Identifier for the Study",
                    "text",
                    "4",
                    "",
                    "Yes",
                    "Topic",
                    "",
                    "CRF; Annotated Case Report Form (acrf.pdf); pages 1",
                    "",
                ],
            ),
        ]
        for name, variable, expected in cases:
            with self.subTest(benchmark=name):
                benchmark = generate.BENCHMARKS / name
                path = benchmark / "expected/define.xml"
                rendered, count, subjects = generate.render_files(
                    [path], "output", benchmark, set(), {}
                )
                document = html_tree.fromstring(rendered)
                row = document.xpath(
                    f'//table[@class="define-table"]/tbody/tr[td[1]="{variable}"]'
                )[0]
                self.assertEqual(
                    ([cell.text_content() for cell in row], count, subjects),
                    (expected, 0, set()),
                )
                self.assertEqual(
                    document.xpath('//details[@class="file-source"]//code')[
                        0
                    ].text_content(),
                    path.read_text(),
                )
                self.assertEqual(document.xpath("//details/@open"), [])

    def test_define_xml_resolves_value_conditions_and_codelist_values(self):
        benchmark = generate.BENCHMARKS / "adam-adlb-metadata"
        rendered, _, _ = generate.render_files(
            [benchmark / "expected/define.xml"], "output", benchmark, set(), {}
        )
        document = html_tree.fromstring(rendered)
        row = document.xpath(
            '//table[caption="ADLB.AVALC value-level metadata"]/tbody/tr'
        )[0]
        self.assertEqual(
            [cell.text_content() for cell in row],
            [
                "PARAMCD = HCG",
                "AVALC",
                "Analysis Value (C)",
                "text",
                "8",
                "",
                "Yes",
                "",
                "Pregnancy Test Result (CL.HCGRS)\nNEGATIVE\nPOSITIVE",
                "Predecessor; LB.LBSTRESC",
                "",
            ],
        )

    def test_html_files_render_in_isolated_frames_and_keep_original_source(self):
        # Read authored HTML preview cases from their fixture home.
        fixtures = HERE / "fixtures/benchmark_docs"
        cases = json.loads((fixtures / "html-previews.json").read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for case in cases:
                source = (fixtures / case["source"]).read_text()
                for suffix in case["suffixes"]:
                    with self.subTest(source=case["source"], suffix=suffix):
                        self.check_html_preview(root, source, suffix, case["expected"])

    def test_define_xml_resolves_dataset_comments_and_document_references(self):
        fixtures = HERE / "fixtures/benchmark_docs"
        cases = json.loads((fixtures / "define-summaries.json").read_text())
        for case in cases:
            with self.subTest(benchmark=case["benchmark"]):
                benchmark = generate.BENCHMARKS / case["benchmark"]
                rendered, _, _ = generate.render_files(
                    [benchmark / "expected/define.xml"], "output", benchmark, set(), {}
                )
                document = html_tree.fromstring(rendered)
                self.assertEqual(
                    document.xpath('//div[@class="define-document"]/section/p')[
                        0
                    ].text_content(),
                    case["expected"],
                )

    def check_html_preview(self, root, source, suffix, expected):
        path = support.write(root, "expected/report" + suffix, source)
        rendered, count, subjects = generate.render_files(
            [path], "output", root, set(), {}
        )
        document = html_tree.fromstring(rendered)
        frame = document.xpath("//iframe")[0]
        preview = html_tree.fromstring(frame.get("srcdoc"))
        self.assertEqual(
            {
                "title": frame.get("title"),
                "sandbox": frame.get("sandbox"),
                "src": frame.get("src"),
                "row_count": count,
                "subjects": sorted(subjects),
                "headings": preview.xpath("//h1/text()"),
                "parent_active_tags": document.xpath("//h1 | //script"),
                "source_open": document.xpath("//details/@open"),
                "policy": preview.xpath(
                    '//meta[@http-equiv="Content-Security-Policy"]/@content'
                )[0],
                "original_source": document.xpath("//details//code")[0].text_content(),
            },
            {
                "title": "Rendered expected/report" + suffix,
                "original_source": source,
                **expected,
            },
        )

    def test_other_and_malformed_xml_remain_literal(self):
        cases = ("sdtm-dm-odm-snapshot", "negative-odm-unescaped-ampersand")
        for name in cases:
            with self.subTest(benchmark=name):
                benchmark = generate.BENCHMARKS / name
                path = benchmark / "input/odm.xml"
                rendered, _, _ = generate.render_files(
                    [path], "input", benchmark, set(), {}
                )
                document = html_tree.fromstring(rendered)
                self.assertEqual(
                    document.xpath("//pre/code")[0].text_content(), path.read_text()
                )
                self.assertEqual(document.xpath('//table[@class="define-table"]'), [])

    def test_public_host_metadata_and_reviewed_exclusions_are_visible(self):
        active = generate.render_benchmark(
            generate.BENCHMARKS / "sdtm-dm-basic"
        ).decode("ascii")
        self.assertIn("Public entries: Python and R.", active)
        self.assertIn("Passing coverage requires an installed inventory.", active)
        with tempfile.TemporaryDirectory() as temporary:
            case = Path(temporary) / "negative-unknown-window"
            shutil.copytree(generate.BENCHMARKS / case.name, case)
            metadata = yaml.safe_load((case / "benchmark.yaml").read_text())
            metadata["conformance"] = {"excluded": "a reviewed reason"}
            (case / "benchmark.yaml").write_text(yaml.safe_dump(metadata))
            excluded = generate.render_benchmark(case).decode("ascii")
        self.assertIn("Public conformance excluded: a reviewed reason", excluded)

    def test_mapping_uses_project_codelists_beside_the_benchmark(self):
        benchmark = generate.BENCHMARKS / "sdtm-dm-metadata"
        page = generate.render_benchmark(benchmark).decode("ascii")
        mapping = page.split('id="mapping-spec"', 1)[1].split('role="tabpanel"', 1)[0]
        self.assertIn(">Country Codes (ISO 3166, version 2020)</td>", mapping)
        self.assertIn(
            '<option value="mapping-sheet-codelists">Codelists</option>', mapping
        )
        self.assertIn(">Demographics</td>", mapping)
        self.assertIn(">C16576</td>", mapping)

    def test_source_values_and_yaml_are_exact(self):
        content = DashboardContent(benchmark_page().decode("ascii"))
        expected_cells = []
        for relative in ["input/ae.csv", "input/dm.csv", "expected/adae.csv"]:
            path = BENCHMARK / relative
            with path.open(newline="") as stream:
                rows = list(csv.reader(stream))
            expected_cells.extend(value for row in rows[1:] for value in row)
        self.assertEqual(content.cells, expected_cells)
        spec_lines = (BENCHMARK / "spec.yaml").read_text().splitlines()
        python_lines = ["import yamaa", "", 'yamaa.spec("spec.yaml").run().output']
        r_lines = ["library(yamaa)", "", 'yamaa_spec("spec.yaml")$run()$output']
        solution_lines = [
            line
            for name in generate.SOLUTION_FILES
            for line in (SOLUTIONS / name).read_text().splitlines()
        ]
        self.assertEqual(
            content.code, spec_lines + r_lines + python_lines + solution_lines
        )
        self.assertEqual(
            content.sections,
            [
                "readme",
                "specification",
                "inputs",
                "outputs",
                "your-review",
                "discussion",
            ],
        )
        self.assertEqual(content.downloads, [])
        self.assertEqual(
            [tab["id"] for tab in content.tabs],
            [
                "tab-comments",
                "tab-mapping-spec",
                "tab-prompt",
                "tab-code",
                "tab-solution",
            ],
        )
        self.assertEqual(
            [pane["aria-label"] for pane in content.file_panes],
            ["input/ae.csv", "input/dm.csv", "expected/adae.csv"],
        )
        self.assertTrue(all("hidden" not in pane for pane in content.file_panes))

    def test_metadata_taxonomy_moves_above_title_and_short_summary_is_one_column(self):
        # adam-advs-bmi's README grew past the short-summary line cap
        # (PR #1097 rewrote it to the data-contract format); the corpus'
        # remaining short-summary benchmark is negative-str-pad-width.
        benchmark = generate.BENCHMARKS / "negative-str-pad-width"
        page = generate.render_benchmark(benchmark).decode("ascii")
        header, _, summary = page.partition('<section id="readme"')
        self.assertIn('<p class="eyebrow">ADaM.ADLB</p>', header)
        self.assertIn("<h1>Reject a Nonnumeric Padding Width</h1>", header)
        self.assertNotIn("text-transform: uppercase", page)
        self.assertIn('<div class="prose prose-short"', summary)
        self.assertNotIn("Standard:", summary.partition("</section>")[0])
        self.assertNotIn("Domain:", summary.partition("</section>")[0])
        self.assertEqual(
            generate.readme_body_line_count(
                benchmark.joinpath("README.md").read_text()
            ),
            10,
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
            self.assertNotEqual(generate.render_benchmark(copied), benchmark_page())
            output = folder / "output"
            output.mkdir()
            page = output / (BENCHMARK.name + ".html")
            command = [BENCHMARK.name, "--check", "--output-dir", str(output)]
            self.assertEqual(support.run_main(generate.main, *command)[0], 1)
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
            self.assertEqual(support.run_main(generate.main, *command)[0], 0)
            page.write_bytes(b"outdated page")
            self.assertEqual(support.run_main(generate.main, *command)[0], 1)
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

    def test_root_relative_links_and_images_are_not_prefixed(self):
        _, readme = generate.render_readme(
            "# Benchmark\n\n[Abs](/absolute/path) ![Img](/img.png) [Rel](rel/path)",
            "https://benchmark.org/fixture",
        )
        self.assertIn('href="/absolute/path"', readme)
        self.assertIn('href="/img.png"', readme)
        self.assertIn('href="https://benchmark.org/fixture/rel/path"', readme)
        self.assertNotIn("fixture//absolute", readme)
        self.assertNotIn("fixture//img", readme)

    def test_taxonomy_lives_in_metadata_and_not_in_summary_prose(self):
        metadata = generate.load_metadata(BENCHMARK)
        self.assertEqual((metadata["standard"], metadata["domain"]), ("ADaM", "ADAE"))
        source = "# Benchmark\n\nBody.\n"
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
        self.assertIn("<div><dt>rule</dt><dd>OUTPUT-3</dd></div>", page)
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
        page = benchmark_page().decode("ascii")
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
        page = benchmark_page().decode("ascii")
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

    def test_code_panel_generates_host_calls_and_edits_the_metadata_owner(self):
        benchmark = generate.BENCHMARKS / "sdtm-dm-basic"
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertIn('id="code"', page)
        self.assertIn('data-filename="Python"', page)
        self.assertIn('data-filename="R"', page)
        self.assertIn("import yamaa", page)
        self.assertIn("library(yamaa)", page)
        self.assertIn(
            '<a class="edit-button" href="https://github.com/elong0527/yamaa/edit/main/benchmarks/sdtm-dm-basic/benchmark.yaml">Edit</a>',
            page,
        )
        self.assertIn(
            '<a class="edit-button" href="https://github.com/elong0527/yamaa/edit/main/benchmarks/sdtm-dm-basic/benchmark.yaml">Edit</a>',
            page,
        )
        self.assertIn('id="code-select"', page)
        codeless = generate.BENCHMARKS / "negative-ambiguous-type"
        plain = generate.render_benchmark(codeless).decode("ascii")
        self.assertNotIn('id="code"', plain)

    def test_material_pane_offers_a_picker_only_between_documents(self):
        documents = [
            (
                "analysis.py",
                "code-pane-analysis-py",
                '<div id="code-pane-analysis-py"></div>',
            ),
            ("figure.R", "code-pane-figure-r", '<div id="code-pane-figure-r"></div>'),
        ]
        pane = generate.render_material_pane(
            "code", "yamaa code", "Runs the specification", "File", documents
        )
        self.assertIn('<div role="tabpanel" id="code" class="material-pane"', pane)
        self.assertIn('<select id="code-select" data-documents>', pane)
        self.assertIn(
            '<option value="code-pane-analysis-py">analysis.py</option>', pane
        )
        self.assertIn('<option value="code-pane-figure-r">figure.R</option>', pane)
        # Without the script every document shows, so none starts hidden.
        self.assertNotIn("hidden", pane)
        single = generate.render_material_pane(
            "code", "yamaa code", "Runs the specification", "File", documents[:1]
        )
        self.assertNotIn("<select", single)

    def test_review_survey_asks_three_questions_with_fixed_choices(self):
        def survey(name):
            page = generate.render_benchmark(generate.BENCHMARKS / name).decode("ascii")
            questions = [
                (
                    name_,
                    target,
                    label,
                    html.unescape(prompt),
                    tuple(re.findall(r'name="[^"]+" value="([^"]+)"', choices)),
                )
                for label, target, prompt, choices in re.findall(
                    r'<fieldset class="review-question" data-label="([^"]+)">'
                    r'<legend><a href="([^"]+)">.*?<span class="review-prompt">'
                    r"([^<]+)</span></legend>(.*?)</fieldset>",
                    page,
                )
                for name_ in re.findall(r'name="([^"]+)"', choices)[:1]
            ]
            return page, questions

        agreement = ("Agree", "Disagree", "Need clarification")
        readiness = (
            "readiness",
            generate.REPOSITORY + "/blob/main/benchmarks/README.md#lifecycle",
            "Overall",
            "Would you agree that the benchmark is ready to finalize?",
            ("Ready", "Ready after minor fixes", "Needs major changes", "Unsure"),
        )
        inputs = (
            "inputs",
            "#inputs",
            "Inputs",
            "Would you agree that the inputs and outputs represent real "
            "data structures collected in clinical trials?",
            agreement,
        )

        task = (
            "task",
            "#readme",
            "Task",
            "Would you agree that the benchmark use case represents real "
            "data derivation in clinical trials?",
            agreement,
        )

        cases = (
            (BENCHMARK.name, [task, inputs, readiness]),
            ("sdtm-mh-ongoing-conditions", [task, inputs, readiness]),
            ("send-bw-sendig311", [task, inputs, readiness]),
            ("schema-source-ordinal", [task, inputs, readiness]),
            (
                "negative-ambiguous-type",
                [
                    (
                        "task",
                        "#readme",
                        "Task",
                        "Is this a specification error a study team could "
                        "realistically make, and should the build stop on it "
                        "rather than continue?",
                        agreement,
                    ),
                    (
                        "inputs",
                        "#inputs",
                        "Inputs",
                        "Do the inputs trigger the failure for the reason the "
                        "summary gives?",
                        agreement,
                    ),
                    readiness,
                ],
            ),
        )
        for name, expected in cases:
            with self.subTest(case=name):
                page, questions = survey(name)
                self.assertEqual(questions, expected, name)
                # One free-text box, a script-only form, and GitHub's composer.
                self.assertEqual(page.count('name="suggestions"'), 1, name)
                self.assertIn(
                    'placeholder="Suggestions to finalize the benchmark"', page, name
                )
                self.assertIn(
                    f'<form class="review-form" data-benchmark="{name}" '
                    'data-review-url="https://github.com/elong0527/yamaa/'
                    'discussions/new?category=comments" novalidate hidden>',
                    page,
                )

        page, _ = survey(BENCHMARK.name)
        ask = page.split('<div class="review-ask-body">', 1)[1].split("</div>", 1)[0]
        self.assertEqual(
            ask,
            "<p>As a clinical trial statistician or programmer, answer three short "
            'questions in <a href="#your-review">Your review</a>: the task, the '
            "inputs and outputs, and whether this benchmark is ready to finalize. "
            "Propose a fix with the <strong>Edit</strong> button on any source "
            'file, or discuss it in <a href="#comments">Comments</a>.</p>'
            '<p class="review-ask-note">'
            "The other tabs beside Comments are background for assessment, not "
            "under review.</p>",
        )
        # The summary is challenged too, so it carries the chip with the data.
        self.assertEqual(page.count('<span class="review-chip">Under review</span>'), 3)
        self.assertIn(
            'Edit</a><span class="review-chip">Under review</span></span>'
            '<span class="panel-caption">What this benchmark means</span>',
            page,
        )

    def test_every_source_shown_has_an_edit_link_to_a_file_that_exists(self):
        """The review ask points at the Edit buttons, so no source may lack one."""
        prefix = generate.REPOSITORY + "/edit/main/"
        for benchmark in sorted(generate.BENCHMARKS.iterdir()):
            if not (
                generate.benchmark_has_spec(benchmark)
                and (benchmark / "README.md").is_file()
            ):
                continue
            page = generate.render_benchmark(benchmark).decode("ascii")
            headings = re.findall(
                r'<div class="file-heading[^"]*">(.*?)</div>', page, re.DOTALL
            )
            headings += [
                header
                for header in re.findall(
                    r'<header class="panel-header">(.*?)</header>', page, re.DOTALL
                )
                if 'id="readme-heading"' in header
                or 'id="expected-failure-heading"' in header
            ]
            self.assertGreater(len(headings), 2, benchmark.name)
            for heading in headings:
                with self.subTest(benchmark=benchmark.name, heading=heading[:80]):
                    links = re.findall(
                        r'<a class="edit-button" href="([^"]+)">Edit</a>', heading
                    )
                    self.assertEqual(len(links), 1)
                    self.assertTrue(links[0].startswith(prefix), links[0])
                    target = generate.ROOT / unquote(links[0][len(prefix) :])
                    self.assertTrue(target.is_file(), links[0])

    def test_comments_lead_the_tabs_and_the_rest_are_not_under_review(self):
        content = DashboardContent(benchmark_page().decode("ascii"))
        comments, *materials = content.tabs
        self.assertEqual(comments["aria-controls"], "comments")
        self.assertEqual(comments["aria-selected"], "true")
        self.assertIn("review-tab", comments["class"].split())
        for tab in materials:
            self.assertEqual(tab["aria-selected"], "false")
            self.assertEqual(tab["aria-describedby"], "not-under-review")
            self.assertIn("reference-tab", tab["class"].split())

    def test_prompts_and_solutions_come_from_the_agent_evaluation(self):
        page = benchmark_page().decode("ascii")
        prompts = generate.HARBOR / "prompts" / BENCHMARK.name
        self.assertIn(
            '<option value="prompt-pane-full">Full &#8212; every rule an expected cell depends on</option>',
            page,
        )
        for tier in ("full", "conventions", "brief"):
            with self.subTest(tier=tier):
                self.assertIn(f'id="prompt-pane-{tier}"', page)
                self.assertIn(
                    '<a class="edit-button" href="https://github.com/elong0527/yamaa/edit/main/'
                    f'evaluations/harbor/prompts/adam-adae-death/{"full" if tier == "brief" else tier}.md">Edit</a>',
                    page,
                )
        self.assertIn(
            'DTHFL is "Y" or has no value.', (prompts / "conventions.md").read_text()
        )
        self.assertIn("<p>DTHFL is &quot;Y&quot; or has no value.</p>", page)
        for name in generate.SOLUTION_FILES:
            self.assertIn(f'data-filename="solutions/adam-adae-death/{name}"', page)
            self.assertIn(
                '<a class="edit-button" href="https://github.com/elong0527/yamaa/edit/main/'
                f'evaluations/harbor/solutions/adam-adae-death/{name}">Edit</a>',
                page,
            )

    def test_prompt_text_is_read_from_the_harbor_folder_by_benchmark_name(self):
        with tempfile.TemporaryDirectory() as directory:
            harbor = Path(directory)
            folder = harbor / "prompts" / BENCHMARK.name
            folder.mkdir(parents=True)
            (folder / "full.md").write_text(
                "Following CDISC ADaM standards, build <ADAE> & keep *every* row.\n\n"
                "The output dataset should contain the following columns in this order: ID.\n\n"
                "Use the collected ID.\n\n"
                "Read the source datasets from /app/input and save /app/output/adae.csv.\n",
                encoding="utf-8",
            )
            with mock.patch.object(generate, "HARBOR", harbor):
                page = generate.render_benchmark(BENCHMARK).decode("ascii")
        self.assertIn("build &lt;ADAE&gt; &amp; keep <em>every</em> row.</p>", page)
        # The brief is derived without a committed source file.
        self.assertIn('id="prompt-select"', page)
        self.assertIn('id="prompt-pane-brief"', page)
        self.assertNotIn('id="tab-solution"', page)

    def test_prompt_links_follow_the_readme_rules(self):
        with tempfile.TemporaryDirectory() as directory:
            harbor = Path(directory)
            folder = harbor / "prompts" / BENCHMARK.name
            folder.mkdir(parents=True)
            (folder / "full.md").write_text(
                "Following CDISC ADaM standards, see [the notes](notes.md), "
                "![a chart](https://example.org/c.png), and [CDISC](https://www.cdisc.org).\n\n"
                "The output dataset should contain the following columns in this order: ID.\n\n"
                "Use the collected ID.\n\n"
                "Read the source datasets from /app/input and save /app/output/adae.csv.\n",
                encoding="utf-8",
            )
            with mock.patch.object(generate, "HARBOR", harbor):
                page = generate.render_benchmark(BENCHMARK).decode("ascii")
        prompt = page.split('id="prompt-pane-full"', 1)[1].split("</div></div>", 1)[0]
        source = generate.REPOSITORY + "/blob/main/evaluations/harbor/prompts/"
        self.assertIn(
            f'<a href="{source}adam-adae-death/notes.md">the notes</a>', prompt
        )
        # An image would make the page fetch it; it shows as a link instead.
        self.assertIn('<a href="https://example.org/c.png">a chart</a>', prompt)
        self.assertNotIn("<img", prompt)
        self.assertIn('<a href="https://www.cdisc.org">CDISC</a>', prompt)

    def test_materials_a_benchmark_lacks_have_no_tab(self):
        def tab_ids(name):
            page = generate.render_benchmark(generate.BENCHMARKS / name).decode("ascii")
            return [tab["id"] for tab in DashboardContent(page).tabs]

        self.assertEqual(
            tab_ids("negative-ambiguous-type"), ["tab-comments", "tab-mapping-spec"]
        )
        # A schema benchmark has no Harbor prompt or solutions, so it
        # carries neither tab and lists no solution files.
        self.assertEqual(
            tab_ids("schema-adsl-age-quality"),
            ["tab-comments", "tab-mapping-spec", "tab-code"],
        )
        page = generate.render_benchmark(
            generate.BENCHMARKS / "schema-adsl-age-quality"
        ).decode("ascii")
        self.assertEqual(re.findall(r'data-filename="(solutions/[^"]+)"', page), [])

    def test_multi_level_spec_renders_panes_with_resolved_default(self):
        # The committed benchmark no longer carries its resolved history
        # file, so stage it in a copied folder and render the copy.
        source = generate.BENCHMARKS / "schema-inheritance"
        with tempfile.TemporaryDirectory() as directory:
            benchmark = (Path(directory) / source.name).resolve()
            shutil.copytree(source, benchmark)
            (benchmark / "expected" / "spec_resolved.yaml").write_text(
                SCHEMA_INHERITANCE_RESOLVED_SPEC, encoding="utf-8"
            )
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
                    "R",
                    "Python",
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
                with self.subTest(path=path):
                    self.assertIn(f'data-edit-url="{base}/{path}"', page)
            self.assertIn(
                'fileHeading.className = "file-heading spec-file-heading"', page
            )
            self.assertIn("edit.href = active.dataset.editUrl", page)
            self.assertNotIn('aria-label="expected/spec_resolved.yaml"', page)
            self.assertNotIn('id="section-select"', page)
            base = "https://github.com/elong0527/yamaa/edit/main/benchmarks/schema-inheritance"
            for target in (
                "spec_organization.yaml",
                "spec_compound.yaml",
                "spec_study.yaml",
            ):
                with self.subTest(target=target):
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

    def test_gallery_categories_use_the_authored_dataset_type(self):
        # Metadata owns the gallery taxonomy even when the entry omits a
        # domain, is a child spec, or the README has a different heading.
        for name, domain in (
            ("schema-inheritance", "ADLB"),
            ("negative-version-mismatch", "ADSL"),
            ("sdtm-dm-race-ethnicity", "DM"),
            ("send-bw-sendig311", "BW"),
        ):
            with self.subTest(name=name):
                _, category = generate.describe_benchmark(generate.BENCHMARKS / name)
                self.assertEqual(category, domain)

    def test_producer_beside_the_entry_renders_as_a_second_spec(self):
        benchmark = generate.BENCHMARKS / "sdtm-dm-race-ethnicity"
        entry, chain = generate.benchmark_entry(benchmark)
        self.assertEqual(entry.name, "spec_suppdm.yaml")
        self.assertEqual([path.name for path in chain], ["spec_dm.yaml"])
        page = generate.render_benchmark(benchmark).decode("ascii")
        self.assertEqual(
            re.findall(r'data-filename="([^"]+)"', page),
            [
                "spec_dm.yaml",
                "spec_suppdm.yaml",
                "prompts/sdtm-dm-race-ethnicity/full.md",
                "prompts/sdtm-dm-race-ethnicity/conventions.md",
                "prompts/sdtm-dm-race-ethnicity/brief.md",
                "R",
                "Python",
                "solutions/sdtm-dm-race-ethnicity/result.R",
                "solutions/sdtm-dm-race-ethnicity/result.py",
                "solutions/sdtm-dm-race-ethnicity/result.sas",
            ],
        )
        self.assertIn('<span class="panel-caption">2 spec files</span>', page)
        # RACE is a DM column: only the producer's columns can label it.
        self.assertIn('<th scope="col" class="derived" title="Race">RACE</th>', page)

    def test_linked_specs_ignores_a_parent_that_is_not_a_path(self):
        # A malformed spec whose parent is a number, null, a list, a
        # mapping or empty names no parent. Iterating it raised TypeError,
        # which escapes main's per-benchmark (OSError, ValueError,
        # YAMLError) handler and aborts the whole dashboard build.
        for parent in (42, None, 1.5, "", ["a.yaml"], {"spec.yaml": True}):
            with self.subTest(parent=parent):
                self.assertEqual(generate.linked_specs({"parent": parent}), [])
        self.assertEqual(generate.linked_specs({"parent": "a.yaml"}), ["a.yaml"])
        # Each fragment an `include` list names is linked too, after the
        # parent; an entry that is not a path names nothing.
        self.assertEqual(
            generate.linked_specs(
                {"parent": "a.yaml", "include": ["b.yaml", 7, "", "c.yaml"]}
            ),
            ["a.yaml", "b.yaml", "c.yaml"],
        )
        self.assertEqual(generate.linked_specs({"include": "b.yaml"}), [])

    def test_scalar_parent_and_columns_do_not_abort_rendering(self):
        source = generate.BENCHMARKS / "sdtm-dm-race-ethnicity"
        with tempfile.TemporaryDirectory() as directory:
            benchmark = (Path(directory) / source.name).resolve()
            shutil.copytree(source, benchmark)
            for name, key in (
                ("spec_dm.yaml", "parent"),
                ("spec_suppdm.yaml", "columns"),
            ):
                path = benchmark / name
                document = yaml.safe_load(path.read_text(encoding="utf-8"))
                document[key] = 42
                path.write_text(yaml.safe_dump(document), encoding="utf-8")
            page = generate.render_benchmark(benchmark).decode("ascii")
            self.assertIn('<span class="panel-caption">2 spec files</span>', page)

    def test_dashboard_badge_is_not_rendered_on_its_own_page(self):
        page = generate.render_benchmark(generate.BENCHMARKS / "sdtm-dm-basic").decode(
            "ascii"
        )
        self.assertNotIn("badge/Dashboard", page)
        self.assertIn("Basic Demographics", page)

    def test_dashboard_lifecycle_requires_metadata_and_never_defaults_from_readme(self):
        source = generate.BENCHMARKS / "sdtm-dm-basic"
        with tempfile.TemporaryDirectory() as temporary:
            benchmark = Path(temporary) / source.name
            shutil.copytree(source, benchmark)
            metadata = benchmark / "benchmark.yaml"
            metadata.write_text("lifecycle: reviewed\nstandard: SDTM\ndomain: DM\n")
            page = generate.render_benchmark(benchmark).decode("ascii")
            self.assertIn('alt="Lifecycle: reviewed"', page)
            metadata.unlink()
            with self.assertRaisesRegex(ValueError, "benchmark.yaml"):
                generate.render_benchmark(benchmark)

    def test_generated_calls_preserve_multifile_order_and_project(self):
        snippets = dict(
            generate.benchmark_code(generate.BENCHMARKS / "sdtm-dm-race-ethnicity")
        )
        self.assertEqual(
            snippets["Python"],
            'import yamaa\n\nyamaa.spec("spec_dm.yaml").run().save()\nyamaa.spec("spec_suppdm.yaml").run().output\n',
        )
        self.assertEqual(
            snippets["R"],
            'library(yamaa)\n\nyamaa_spec("spec_dm.yaml")$run()$save()\nyamaa_spec("spec_suppdm.yaml")$run()$output\n',
        )
        snippets = dict(
            generate.benchmark_code(generate.BENCHMARKS / "schema-functions")
        )
        self.assertEqual(
            snippets["Python"],
            'import yamaa\n\nyamaa.project("project.yaml").spec("TEST").run().output\n',
        )
        self.assertEqual(
            snippets["R"],
            'library(yamaa)\n\nyamaa_project("project.yaml")$spec("TEST")$run()$output\n',
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
        reviewed = generate.render_benchmark(
            generate.BENCHMARKS / "adam-adlb-ordered-sum"
        ).decode("ascii")
        self.assertIn(
            '<img src="https://img.shields.io/badge/Lifecycle-reviewed-yellow"',
            reviewed,
        )
        self.assertIn('alt="Lifecycle: reviewed"', reviewed)
        self.assertIn(
            '<h2 id="readme-heading">Summary</h2><a class="lifecycle-badge"',
            reviewed,
        )
        self.assertIn(
            '<a class="lifecycle-badge" href="https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle">',
            reviewed,
        )
        draft = generate.render_benchmark(
            generate.BENCHMARKS / "schema-structure"
        ).decode("ascii")
        self.assertIn(
            '<img src="https://img.shields.io/badge/Lifecycle-draft-lightgrey"',
            draft,
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
        page = benchmark_page().decode("ascii")
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
            with self.subTest(name=name):
                self.assertIn(f'href="{name}.html"', text)

    def test_gallery_counts_groups_in_the_same_order_as_sections_and_jumps(self):
        # The group table is the figure readers quote, so it is counted from
        # the suite rather than typed into the overview, where it would drift.
        text = generate.render_index(
            [
                ("negative-adsl-one", "Reject", "ADSL"),
                ("schema-adsl-two", "Configure", "ADSL"),
                ("adam-adsl-three", "Derive", "ADSL"),
                ("sdtm-dm-four", "Tabulate", "DM"),
                ("send-bw-five", "Tabulate nonclinical weights", "BW"),
            ]
        ).decode("ascii")
        expected = [("sdtm", "2"), ("adam", "1"), ("schema", "1"), ("negative", "1")]
        self.assertEqual(
            {
                "table": re.findall(r"^\| `([^`]+)` \| (\d+) \|", text, re.M),
                "jumps": re.findall(r'<a href="#([^\"]+)">\w+ \((\d+)\)</a>', text),
                "sections": re.findall(
                    r"^## (\w+) \{:[^\n]+\}\n\n[^\n]+>(\d+) benchmarks?\.",
                    text,
                    re.M,
                ),
            },
            {"table": expected, "jumps": expected, "sections": expected},
        )
        self.assertIn('href="send-bw-five.html"', text.split("## adam")[0])

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
            [("adam-adsl-one", "ADaM ADSL: derive a flag", "ADSL")]
        ).decode("ascii")
        # Front matter and Markdown headings: MkDocs renders this page with the
        # site header, navigation, and palette, so it must not be a whole
        # document of its own.
        self.assertTrue(text.startswith("---\ntitle: Benchmark\n"))
        self.assertIn("\nhide:\n  - actions\n", text)
        self.assertNotIn("<!doctype html>", text)
        self.assertNotIn("<body", text)
        self.assertIn("\n# Benchmark\n", text)
        self.assertIn("\n## adam {: #adam }\n", text)
        self.assertIn("\n### ADSL\n", text)
        self.assertIn("1 benchmark, generated from", text)

    def test_gallery_lists_rejected_benchmarks_apart_from_positive_ones(self):
        entries = [
            ("adam-adsl-one", "ADaM ADSL: derive a flag", "ADSL"),
            ("negative-adsl-two", "ADaM ADSL: reject a flag", "ADSL"),
        ]
        text = generate.render_index(entries).decode("ascii")
        positive = text.index("{: #adam }")
        negative = text.index("{: #negative }")
        self.assertLess(positive, negative)
        self.assertLess(text.index("adam-adsl-one.html"), negative)
        self.assertLess(negative, text.index("negative-adsl-two.html"))
        # One shared domain heading per group, not one shared between them.
        self.assertEqual(text.count("### ADSL\n"), 2)
        self.assertIn('<a href="#adam">adam (1)</a>', text)
        self.assertIn('<a href="#negative">negative (1)</a>', text)

    def test_gallery_omits_a_group_with_no_benchmarks(self):
        text = generate.render_index(
            [("adam-adsl-one", "ADaM ADSL: derive", "ADSL")]
        ).decode("ascii")
        self.assertEqual(re.findall(r"^## (\w+) \{:", text, re.M), ["adam"])
        self.assertEqual(re.findall(r"^\| `([^`]+)`", text, re.M), ["adam"])

    def test_gallery_has_one_subsection_per_dataset_type_in_each_group(self):
        entries = [
            (name, *generate.describe_benchmark(generate.BENCHMARKS / name))
            for name in (
                "schema-inheritance",
                "schema-structure",
                "schema-functions",
                "negative-version-mismatch",
            )
        ]
        text = generate.render_index(entries).decode("ascii")
        self.assertEqual(
            re.findall(r"^#{2,3} ([^\n]+)", text, re.M),
            [
                "Start here",
                "All benchmarks",
                "schema {: #schema }",
                "ADLB",
                "TEST",
                "negative {: #negative }",
                "ADSL",
            ],
        )
        self.assertCountEqual(
            re.findall(r'<li><a href="([^\"]+)\.html">', text),
            [name for name, _, _ in entries],
        )

    def test_multi_level_spec_renders_panes_without_retired_resolved_output(self):
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
                "R",
                "Python",
            ],
        )
        self.assertIn("Choose specification document", page)
        self.assertIn('<span class="panel-caption">3 spec files</span>', page)
        base = (
            "https://github.com/elong0527/yamaa/edit/main/benchmarks/schema-inheritance"
        )
        for path in [
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_study.yaml",
        ]:
            with self.subTest(path=path):
                self.assertIn(f'data-edit-url="{base}/{path}"', page)
        self.assertIn('fileHeading.className = "file-heading spec-file-heading"', page)
        self.assertIn("edit.href = active.dataset.editUrl", page)
        self.assertNotIn('aria-label="expected/spec_resolved.yaml"', page)
        self.assertNotIn('id="section-select"', page)
        base = (
            "https://github.com/elong0527/yamaa/edit/main/benchmarks/schema-inheritance"
        )
        for target in (
            "spec_organization.yaml",
            "spec_compound.yaml",
            "spec_study.yaml",
        ):
            with self.subTest(target=target):
                self.assertIn(
                    f'<a class="edit-button" href="{base}/{target}">Edit</a>', page
                )
        self.assertNotIn('data-filename="spec_resolved.yaml"', page)
        self.assertIn(
            '<header class="section-header"><h2 id="schema-heading">YAML specification</h2>',
            page,
        )


if __name__ == "__main__":
    unittest.main()
