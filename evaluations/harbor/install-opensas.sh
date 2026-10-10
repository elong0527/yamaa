#!/usr/bin/env bash
# Install an existing release. Its reference, version and notices are pinned.
set -euo pipefail
case "${1:?Pass the Docker target architecture}" in
  amd64) asset=amd ;;
  arm64) asset=arm ;;
  *) echo "Unsupported opensas architecture: $1" >&2; exit 1 ;;
esac
curl --fail --location --retry 3 \
  "https://github.com/kirha-ai/opensas/releases/download/v0.6.6/sas-v0.6.6-${asset}" \
  --output /usr/local/bin/opensas
chmod 755 /usr/local/bin/opensas
test "$(opensas --version)" = "opensas v0.6.6"
