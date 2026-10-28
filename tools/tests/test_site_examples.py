"""The YAML examples on the site validate against their file types' JSON
Schemas (tools/validation/check_site_examples.py, #3454): which blocks the
check reads, the `# fragment` convention and the `schema_version` switch."""

import json
import tempfile
import unittest
from pathlib import Path

from tools.commands import repository
from tools.paths import ROOT
from tools.tests import support
from tools.validation import check_site_examples as site

# A file type's JSON Schema, small enough to read beside the blocks below.
SCHEMA = {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "type": "object",
    "required": ["yamaa", "type"],
    "properties": {
        "yamaa": {"const": "1.0"},
        "type": {"const": "spec"},
        "columns": {
            "type": "array",
            "items": {"type": "object", "required": ["name"]},
        },
    },
    "additionalProperties": False,
}
VALID = 'yamaa: "1.0"\ntype: spec\ncolumns:\n  - name: AVAL\n'
INVALID = 'yamaa: "1.0"\ntype: spec\ndomian: ADSL\n'
UNEXPECTED = (
    "the spec file at $: Additional properties are not allowed "
    "('domian' was unexpected)"
)
OLD = 'schema_version: "1.0"\ndomain: ADSL\n'
PAGE = "docs/articles/page.md"


def page(*blocks):
    """A page with each block fenced after a heading; the first block's text
    starts on line 4."""
    return "# Page\n" + "".join(f"\n```yaml\n{block}```\n" for block in blocks)


class Blocks(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="yamaa-site-examples-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        support.write(
            self.root, site.JSON_SCHEMAS / "spec.schema.json", json.dumps(SCHEMA)
        )

    def verdicts(self, cases, **options):
        """Check each case's page and compare the check's whole result."""
        for case, text, expected in cases:
            with self.subTest(case=case):
                support.write(self.root, PAGE, text)
                self.assertEqual(site.check(self.root, **options), expected)

    def test_a_whole_file_is_validated_against_its_type_schema(self):
        self.verdicts(
            (
                ("valid", page(VALID), ([], 1)),
                ("invalid", page(INVALID), ([f"{PAGE}:4: {UNEXPECTED}"], 1)),
                (
                    "a member breaks its schema",
                    page('yamaa: "1.0"\ntype: spec\ncolumns:\n  - label: Value\n'),
                    (
                        [
                            f"{PAGE}:4: the spec file at $.columns[0]: 'name' is "
                            "a required property"
                        ],
                        1,
                    ),
                ),
            )
        )

    def test_a_block_that_declares_yamaa_must_parse_and_name_a_file_type(self):
        broken = 'yamaa: "1.0"\ntype: spec\ncolumns:\n  - name: A\n   x: 1\n'
        unparsed = (
            [
                f"{PAGE}:8: the block does not parse as YAML: expected "
                "<block end>, but found '<block mapping start>'"
            ],
            0,
        )
        self.verdicts(
            (
                ("does not parse", page(broken), unparsed),
                (
                    "an indented root does not parse",
                    page("".join(f"  {line}\n" for line in broken.splitlines())),
                    unparsed,
                ),
                (
                    "no type",
                    page('yamaa: "1.0"\ncolumns: []\n'),
                    (
                        [
                            f"{PAGE}:4: the block declares `yamaa:` without "
                            "`type:`; add its file type, or start a partial "
                            "block with `# fragment`"
                        ],
                        0,
                    ),
                ),
                (
                    "unknown type",
                    page('yamaa: "1.0"\ntype: spek\n'),
                    (
                        [
                            f"{PAGE}:4: `type: spek` names no file type with a "
                            f"JSON Schema in {site.JSON_SCHEMAS.as_posix()}/"
                        ],
                        0,
                    ),
                ),
            )
        )

    def test_a_fragment_or_a_block_without_yamaa_is_not_validated(self):
        self.verdicts(
            (
                ("fragment", page(f"# fragment\n{INVALID}"), ([], 0)),
                (
                    "unparsed fragment",
                    page('# fragment\nyamaa: "1.0"\n  - x\n'),
                    ([], 0),
                ),
                ("excerpt", page("columns:\n  - label: Value\n"), ([], 0)),
                (
                    "a malformed excerpt with a nested header",
                    page('outer:\n  yamaa: "1.0"\n  values: [\n'),
                    ([], 0),
                ),
                ("elision", page("...\n"), ([], 0)),
            )
        )

    def test_the_schema_version_switch_refuses_a_0_5_block_only_when_on(self):
        refused = (
            f"{PAGE}:4: the block declares `schema_version`, the 0.5 header; "
            'write it as a 1.0 file, with `yamaa: "1.0"` and `type:`'
        )
        self.verdicts((("off", page(OLD), ([], 0)),), refuse_schema_version=False)
        self.verdicts(
            (
                ("on", page(OLD), ([refused], 0)),
                ("on, a fragment", page(f"# fragment\n{OLD}"), ([refused], 0)),
                ("on, a 1.0 file", page(VALID), ([], 1)),
            ),
            refuse_schema_version=True,
        )

    def test_only_a_fenced_yaml_block_is_read_at_any_indentation(self):
        self.verdicts(
            (
                (
                    "indented in a tab",
                    '# Page\n\n=== "Tab"\n\n    ```yaml\n'
                    + "".join(f"    {line}\n" for line in INVALID.splitlines())
                    + "    ```\n",
                    ([f"{PAGE}:6: {UNEXPECTED}"], 1),
                ),
                (
                    "a deeper fence-like line is the block's text",
                    page(
                        'yamaa: "1.0"\ntype: spec\ncolumns:\n  - name: AVAL\n'
                        "    note: |\n      ```\ndomian: ADSL\n"
                    ),
                    ([f"{PAGE}:4: {UNEXPECTED}"], 1),
                ),
                (
                    "shown as Markdown source",
                    f"# Page\n\n````markdown\n```yaml\n{INVALID}```\n````\n",
                    ([], 0),
                ),
                ("another language", f"# Page\n\n```python\n{INVALID}```\n", ([], 0)),
                (
                    "an indented fence is YAML content",
                    page(
                        'yamaa: "1.0"\ntype: spec\ncolumns:\n- name: |\n    ```\ndomian: ADSL\n'
                    ),
                    ([f"{PAGE}:4: {UNEXPECTED}"], 1),
                ),
            )
        )

    def test_the_check_reads_the_site_pages_and_package_readmes_only(self):
        read = (
            "README.md",
            "rules/README.md",
            PAGE,
            "python/README.md",
            "R/yamaa/README.md",
        )
        # The design record and the release notes keep their 0.5 examples.
        unread = ("docs/design/record.md", "docs/releases/0.7.0.md", "CONTRIBUTING.md")
        for name in read + unread:
            support.write(self.root, name, page(INVALID))
        self.assertEqual(
            site.check(self.root),
            ([f"{name}:4: {UNEXPECTED}" for name in read], len(read)),
        )

    def test_each_yaml_block_of_an_issue_form_is_checked_at_its_line(self):
        form = ".github/ISSUE_TEMPLATE/bug.yml"
        placeholder = (
            "name: Bug report\nbody:\n"
            "  - type: markdown\n    attributes:\n      value: Public data only.\n"
            "  - type: textarea\n    attributes:\n      label: Show it failing\n"
            "      placeholder: |\n        A failing spec:\n        ```yaml\n"
            + "".join(f"        {line}\n" for line in INVALID.splitlines())
            + "        ```\n"
        )
        rendered = (
            "name: Bug report\nbody:\n"
            "  - type: textarea\n    attributes:\n      label: The spec\n"
            "      render: yaml\n      placeholder: |\n"
            + "".join(f"        {line}\n" for line in INVALID.splitlines())
        )
        for case, text, line in (
            ("fenced in a placeholder", placeholder, 12),
            ("a field that renders YAML", rendered, 8),
        ):
            with self.subTest(case=case):
                support.write(self.root, form, text)
                self.assertEqual(
                    site.check(self.root), ([f"{form}:{line}: {UNEXPECTED}"], 1)
                )

    def test_the_command_prints_each_error_and_fails_or_passes_with_a_count(self):
        for case, text, expected in (
            ("fails", page(INVALID), (1, f"ERROR: {PAGE}:4: {UNEXPECTED}\n", "")),
            (
                "passes",
                page(VALID),
                (
                    0,
                    "PASS: whole 1.0 files on the site pages, in the package "
                    "READMEs and in the issue forms validate against their JSON "
                    "Schemas (1 checked).\n",
                    "",
                ),
            ),
        ):
            with self.subTest(case=case):
                support.write(self.root, PAGE, text)
                self.assertEqual(
                    support.run_main(site.main, "--root", str(self.root)), expected
                )


class Checkout(unittest.TestCase):
    def test_the_checkout_examples_validate(self):
        errors, validated = site.check(ROOT)
        self.assertEqual(errors, [])
        self.assertGreater(validated, 0)

    def test_the_repository_check_runs_it(self):
        self.assertIn("tools.validation.check_site_examples", repository.CHECKS)


if __name__ == "__main__":
    unittest.main()
