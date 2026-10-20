"""Build real MkDocs pages to check source freshness, URLs, and link scope."""

import tempfile
import unittest
from pathlib import Path

from lxml import html
from mkdocs.commands.build import build
from mkdocs.config import load_config
from mkdocs.exceptions import Abort

import yaml
from tools.paths import ROOT


class CanonicalPages(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "docs").mkdir()
        (self.root / "rules").mkdir()
        (self.root / "README.md").write_text(
            "# Home\n\n[Rules](rules/README.md)\n", encoding="utf-8"
        )
        (self.root / "rules/README.md").write_text(
            "# Rules\n\n"
            "[Binding](specification/binding.md#requirements)\n\n"
            "[Inventory](../benchmarks/README.md)\n\n"
            "[Reference][owner]\n\n"
            "[owner]: values/types.md?plain=1#requirements\n\n"
            "![Diagram](../docs/diagrams/design.svg)\n\n"
            "[Local](#rules) [Web](https://example.org/) "
            "[Mail](mailto:author@example.org) [Root](/absolute/) "
            "[CDN](//example.org/image.svg)\n\n"
            "`[Code](values/types.md)`\n\n"
            "```markdown\n[Sample](values/types.md)\n```\n",
            encoding="utf-8",
        )
        (self.root / "docs/ordinary.md").write_text(
            "# Ordinary\n\n[Home](index.md) [Rules](articles/rules.md)\n\n"
            "[Contract](../rules/README.md#rules) [Guide](../CONTRIBUTING.md)\n\n"
            "![Logo](../rules/logo.svg)\n",
            encoding="utf-8",
        )
        (self.root / "docs/articles").mkdir()
        (self.root / "docs/articles/nested.md").write_text(
            "# Nested\n\n[Up](../ordinary.md) [Contract](../../rules/README.md)\n",
            encoding="utf-8",
        )
        repository_config = yaml.safe_load((ROOT / "mkdocs.yml").read_text())
        config = {
            "site_name": repository_config["site_name"],
            "repo_url": repository_config["repo_url"],
            "edit_uri": repository_config["edit_uri"],
            "theme": {
                "name": "material",
                "custom_dir": str(ROOT / repository_config["theme"]["custom_dir"]),
                "features": ["content.action.edit", "content.action.view"],
            },
            "markdown_extensions": repository_config["markdown_extensions"],
            "hooks": [str(ROOT / hook) for hook in repository_config["hooks"]],
            "watch": repository_config["watch"],
            "nav": [
                repository_config["nav"][0],
                {"Rules": repository_config["nav"][2]["Reference"][2]["Rules"]},
                {"Ordinary": "ordinary.md"},
                {"Nested": "articles/nested.md"},
            ],
        }
        self.config_path = self.root / "mkdocs.yml"
        self.config_path.write_text(yaml.safe_dump(config), encoding="utf-8")

    def build(self):
        build(load_config(config_file=str(self.config_path), strict=True))

    def page(self, path):
        return (self.root / "site" / path).read_text(encoding="utf-8")

    def test_canonical_sources_keep_urls_and_resolve_links(self):
        self.build()
        home = self.page("index.html")
        rules = self.page("articles/rules/index.html")
        ordinary = self.page("ordinary/index.html")
        repository = "https://github.com/elong0527/yamaa"
        self.assertIn(f'href="{repository}/edit/main/README.md"', home)
        self.assertIn(f'href="{repository}/edit/main/rules/README.md"', rules)
        self.assertIn(f'href="{repository}/raw/main/README.md"', home)
        self.assertIn(f'href="{repository}/raw/main/rules/README.md"', rules)
        for path in (
            "rules/README.md",
            "rules/specification/binding.md#requirements",
            "benchmarks/README.md",
            "rules/values/types.md?plain=1#requirements",
        ):
            self.assertIn(f'href="{repository}/blob/main/{path}"', home + rules)
        self.assertIn(f'src="{repository}/raw/main/docs/diagrams/design.svg"', rules)
        for url in (
            "#rules",
            "https://example.org/",
            "mailto:author@example.org",
            "/absolute/",
            "//example.org/image.svg",
        ):
            self.assertIn(f'href="{url}"', rules)
        self.assertIn("[Code](values/types.md)", rules)
        code_text = html.fromstring(rules).xpath("string(//pre/code)")
        self.assertEqual("[Sample](values/types.md)\n", code_text)
        self.assertIn('href="../"', ordinary)
        self.assertIn('href="../articles/rules/"', ordinary)
        # A docs page's links that leave docs/ name repository files on main.
        self.assertIn(f'href="{repository}/blob/main/rules/README.md#rules"', ordinary)
        self.assertIn(f'href="{repository}/blob/main/CONTRIBUTING.md"', ordinary)
        self.assertIn(f'src="{repository}/raw/main/rules/logo.svg"', ordinary)
        nested = self.page("articles/nested/index.html")
        self.assertIn('href="../../ordinary/"', nested)
        self.assertIn(f'href="{repository}/blob/main/rules/README.md"', nested)
        self.assertFalse((self.root / "docs/index.md").exists())
        self.assertFalse((self.root / "docs/articles/rules.md").exists())

    def test_rebuild_reads_current_sources(self):
        self.build()
        for source in ("README.md", "rules/README.md"):
            with (self.root / source).open("a", encoding="utf-8") as file:
                file.write("\nCurrent source after rebuild.\n")
        self.build()
        for output in ("index.html", "articles/rules/index.html"):
            self.assertIn("Current source after rebuild.", self.page(output))

    def test_repository_sources_build_without_copies(self):
        for source, destination in (
            ("README.md", "index.md"),
            ("rules/README.md", "articles/rules.md"),
        ):
            self.assertFalse((ROOT / "docs" / destination).exists())
            (self.root / source).write_bytes((ROOT / source).read_bytes())
        self.build()

    def test_rule_references_render_from_current_sources(self):
        (self.root / "rules/values").mkdir()
        (self.root / "rules/values/types.md").write_text(
            '# Types\n\n<a id="type-1"></a>\n\n### TYPE-1. A column has one type\n\n'
            "A column has one type.\n",
            encoding="ascii",
        )
        (self.root / "rules/inheritance.md").write_text(
            '# Inheritance\n\n<a id="layer-1"></a>\n\n'
            "### LAYER-1. Layers compose depth first\n\n"
            "Each layer contributes once; see [LAYER-1](#layer-1).\n",
            encoding="ascii",
        )
        self.build()
        repository = "https://github.com/elong0527/yamaa"
        index = self.page("reference/rule-index/index.html")
        self.assertIn(
            f'href="{repository}/blob/main/rules/values/types.md#type-1"', index
        )
        self.assertIn(
            f'href="{repository}/blob/main/rules/inheritance.md#layer-1"', index
        )
        self.assertNotIn("/edit/main/", index)
        self.assertIn("Schema fields", self.page("reference/schema-fields/index.html"))
        summary = self.page("reference/rules-summary/index.html")
        self.assertIn("Rules at a glance", summary)
        self.assertIn("LAYER-1", summary)
        self.assertIn(
            f'href="{repository}/blob/main/rules/inheritance.md#layer-1"', summary
        )
        self.assertNotIn("/edit/main/", summary)
        self.assertFalse((self.root / "rules/reference").exists())
        (self.root / "rules/values/types.md").write_text(
            '# Types\n\n<a id="type-2"></a>\n\n### TYPE-2. A column has one type\n\n'
            "A column has one type.\n",
            encoding="ascii",
        )
        self.build()
        self.assertIn("TYPE-2", self.page("reference/rule-index/index.html"))

    def test_duplicate_pages_fail_with_the_canonical_edit_target(self):
        for destination, message in (
            ("index.md", "Edit README.md"),
            ("articles/rules.md", "Edit rules/README.md"),
            (
                "reference/rule-index.md",
                "rules/reference/rule-index.md is generated",
            ),
            (
                "reference/rules-summary.md",
                "rules/reference/rules-summary.md is generated",
            ),
        ):
            with self.subTest(destination=destination):
                duplicate = self.root / "docs" / destination
                duplicate.parent.mkdir(parents=True, exist_ok=True)
                duplicate.write_text("# Stale copy\n", encoding="utf-8")
                try:
                    with (
                        self.assertLogs("mkdocs", level="ERROR") as logs,
                        self.assertRaises(Abort),
                    ):
                        self.build()
                    self.assertIn(message, "\n".join(logs.output))
                finally:
                    duplicate.unlink()


if __name__ == "__main__":
    unittest.main()
