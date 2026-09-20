#!/bin/sh
# Regenerate every benchmark dashboard into docs/benchmark/ before building or
# serving the MkDocs site. Mirrors the generate step in
# .github/workflows/deploy-docs.yml. Generated HTML is git-ignored; do not
# edit it by hand.
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
python "$ROOT/.github/scripts/benchmark-docs/generate.py" --all --quiet
