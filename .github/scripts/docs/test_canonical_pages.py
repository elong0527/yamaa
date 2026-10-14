"""Build real MkDocs pages to check source freshness, URLs, and link scope."""

import tempfile
import unittest
from pathlib import Path

from mkdocs.commands.build import build
from mkdocs.config import load_config
from mkdocs.exceptions import Abort

import yaml

ROOT = Path(__file__).resolve().parents[3]


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
            "# Ordinary\n\n[Home](index.md) [Rules](articles/rules.md)\n",
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
        self.assertIn("[Sample](values/types.md)", rules)
        self.assertIn('href="../"', ordinary)
        self.assertIn('href="../articles/rules/"', ordinary)
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

    def test_duplicate_pages_fail_with_the_canonical_edit_target(self):
        for destination, source in (
            ("index.md", "README.md"),
            ("articles/rules.md", "rules/README.md"),
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
                    self.assertIn(f"Edit {source}", "\n".join(logs.output))
                finally:
                    duplicate.unlink()


if __name__ == "__main__":
    unittest.main()
