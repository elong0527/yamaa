#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
    bash ca-certificates curl git jq locales procps ripgrep util-linux xz-utils
rm -rf /var/lib/apt/lists/*
locale-gen en_US.UTF-8
useradd --create-home --uid 1001 --shell /bin/bash agent
mkdir -p /app/input /app/output /logs/agent /logs/verifier /logs/artifacts
chown -R agent:agent /app /logs
