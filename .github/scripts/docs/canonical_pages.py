"""Render repository-owned pages without maintaining copies under docs/."""

import os
import sys
from pathlib import Path
from urllib.parse import urljoin, urlsplit

from markdown.extensions import Extension
from markdown.treeprocessors import Treeprocessor
from mkdocs.exceptions import PluginError
from mkdocs.structure.files import File

SOURCES = {"index.md": "README.md", "articles/rules.md": "rules/README.md"}
# Rendered from rules/ and yaml/ on every build; never committed or edited.
GENERATED = {
    "reference/requirements.md": "rules/reference/requirements.md",
    "reference/schema-fields.md": "rules/reference/schema-fields.md",
}
RULE_TOOLS = Path(__file__).resolve().parents[1] / "yaml-validation"


class RepositoryLinks(Treeprocessor):
    def __init__(self, md, source_url):
        super().__init__(md)
        self.source_url = source_url

    def run(self, root):
        for element in root.iter():
            attribute = {"a": "href", "img": "src"}.get(element.tag)
            if attribute is None:
                continue
            destination = element.get(attribute, "")
            url = urlsplit(destination)
            if url.path and not (url.scheme or url.netloc or url.path.startswith("/")):
                source_url = self.source_url
                if element.tag == "img":
                    source_url = source_url.replace("/blob/main/", "/raw/main/", 1)
                element.set(attribute, urljoin(source_url, destination))


class RepositoryLinksExtension(Extension):
    def __init__(self, source_url):
        super().__init__()
        self.source_url = source_url

    def extendMarkdown(self, md):
        # Resolve parsed Markdown links before MkDocs validates site links (0).
        md.treeprocessors.register(
            RepositoryLinks(md, self.source_url), "repository_links", 2
        )


def rule_references(root):
    """Render the requirement and schema-field references from their sources."""
    if str(RULE_TOOLS) not in sys.path:
        sys.path.insert(0, str(RULE_TOOLS))
    from generate_rule_reference import generated

    try:
        references = generated(root)
    except ValueError as error:
        raise PluginError(f"Cannot generate rule references: {error}") from error
    return {
        path.relative_to(root).as_posix(): text for path, text in references.items()
    }


def on_files(files, *, config):
    root = Path(config.config_file_path).parent
    for destination, source in SOURCES.items():
        if files.get_file_from_path(destination) is not None:
            raise PluginError(
                f"Edit {source}; remove the duplicate docs/{destination}."
            )
        file = File.generated(config, destination, abs_src_path=str(root / source))
        file.edit_uri = Path(os.path.relpath(root / source, config.docs_dir)).as_posix()
        files.append(file)
    references = rule_references(root)
    for destination, source in GENERATED.items():
        if files.get_file_from_path(destination) is not None:
            raise PluginError(
                f"{source} is generated; remove the duplicate docs/{destination}."
            )
        # Generated files carry no edit target: edit the contract or schema.
        files.append(File.generated(config, destination, content=references[source]))
    return files


def on_page_markdown(markdown, *, page, config, files):
    # MkDocs renders pages sequentially; keep repository link resolution scoped
    # to the current canonical page, including across serve rebuilds.
    config.markdown_extensions = [
        extension
        for extension in config.markdown_extensions
        if not isinstance(extension, RepositoryLinksExtension)
    ]
    if source := SOURCES.get(page.file.src_uri) or GENERATED.get(page.file.src_uri):
        source_url = f"{config.repo_url.rstrip('/')}/blob/main/{source}"
        config.markdown_extensions.append(RepositoryLinksExtension(source_url))
    return markdown
