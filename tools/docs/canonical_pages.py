"""Render repository-owned pages without maintaining copies under docs/.

Site pages under docs/ may also link to repository files outside docs/ by
relative path, so a copy read at a release tag names that tag's files; the
site, built from main, links them on main.
"""

import os
import posixpath
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


def leaves_docs(page, path):
    """Whether a relative link on a docs page resolves outside docs/."""
    target = posixpath.normpath(posixpath.join(posixpath.dirname(page), path))
    return target == ".." or target.startswith("../")


class RepositoryLinks(Treeprocessor):
    def __init__(self, md, source_url, docs_page=None):
        super().__init__(md)
        self.source_url = source_url
        # A page under docs/ keeps its links to other site pages for MkDocs to
        # resolve; only a link that leaves docs/ names a repository file.
        self.docs_page = docs_page

    def run(self, root):
        for element in root.iter():
            attribute = {"a": "href", "img": "src"}.get(element.tag)
            if attribute is None:
                continue
            destination = element.get(attribute, "")
            url = urlsplit(destination)
            if url.path and not (url.scheme or url.netloc or url.path.startswith("/")):
                if self.docs_page is not None and not leaves_docs(
                    self.docs_page, url.path
                ):
                    continue
                source_url = self.source_url
                if element.tag == "img":
                    source_url = source_url.replace("/blob/main/", "/raw/main/", 1)
                element.set(attribute, urljoin(source_url, destination))


class RepositoryLinksExtension(Extension):
    def __init__(self, source_url, docs_page=None):
        super().__init__()
        self.source_url = source_url
        self.docs_page = docs_page

    def extendMarkdown(self, md):
        # Resolve parsed Markdown links before MkDocs validates site links (0).
        md.treeprocessors.register(
            RepositoryLinks(md, self.source_url, self.docs_page),
            "repository_links",
            2,
        )


def rule_references(root):
    """Render the requirement and schema-field references from their sources."""
    from tools.validation.generate_rule_reference import generated

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
    repository = config.repo_url.rstrip("/")
    if source := SOURCES.get(page.file.src_uri) or GENERATED.get(page.file.src_uri):
        source_url = f"{repository}/blob/main/{source}"
        config.markdown_extensions.append(RepositoryLinksExtension(source_url))
    else:
        root = Path(config.config_file_path).parent
        docs = Path(os.path.relpath(config.docs_dir, root)).as_posix()
        source_url = f"{repository}/blob/main/{docs}/{page.file.src_uri}"
        config.markdown_extensions.append(
            RepositoryLinksExtension(source_url, page.file.src_uri)
        )
    return markdown
