"""Site pages rendered from repository sources instead of files under docs/."""

from tools.rules.model import GENERATED_PAGES

# Site page and the repository file it renders; never copied under docs/.
CANONICAL = {"index.md": "README.md", "articles/rules.md": "rules/README.md"}
# Rendered from rules/ and yaml/ on every build; never committed or edited.
GENERATED = {page: f"rules/{page}" for page in GENERATED_PAGES}
# Written by the benchmark dashboard generator before each build.
GENERATED_DIRECTORIES = ("benchmark/",)
