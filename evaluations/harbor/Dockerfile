# syntax=docker/dockerfile:1.7
#
# Base image for yamaa agent evaluations: the agent's sandbox and the
# verifier. Python and R with common data packages; no yamaa, no benchmark
# files, no network at run time beyond the model API.
#
#   docker build -t yamaa-harbor-env:0.1 evaluations/harbor
#
# Bump the tag in build.py (IMAGE) whenever this file changes.

ARG R_VERSION=4.6.1
ARG UV_VERSION=0.12.21

FROM ghcr.io/astral-sh/uv:${UV_VERSION} AS uv

FROM rocker/r-ver:${R_VERSION}

ARG R_VERSION=4.6.1
ARG CRAN_SNAPSHOT=2026-09-27
ARG PYPI_SNAPSHOT=2026-09-22T00:00:00Z
ARG OPENCODE_VERSION=1.18.33

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=en_US.UTF-8 \
    UV_NO_CACHE=1 \
    PATH=/opt/yamaa-eval/venv/bin:${PATH}

# --- System tools. With curl, bash, stdbuf, node, and npm present, Harbor
# --- skips apt at agent setup; it still installs OpenCode with nvm and npm.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash ca-certificates curl git jq procps ripgrep xz-utils \
        python3 python3-venv nodejs npm \
        libxml2-dev zlib1g-dev libicu-dev \
    && rm -rf /var/lib/apt/lists/*

COPY --from=uv /uv /uvx /usr/local/bin/

# --- Python data packages, resolved as of a fixed date ----------------------
RUN uv venv /opt/yamaa-eval/venv --python /usr/bin/python3 \
    && UV_EXCLUDE_NEWER="${PYPI_SNAPSHOT}" uv pip install \
        --python /opt/yamaa-eval/venv/bin/python \
        numpy pandas polars pyarrow lxml \
    && python -c "import numpy, pandas, polars, pyarrow, lxml"

# --- R data packages from a dated Posit Package Manager snapshot -----------
RUN R -q -e "options(repos = c(CRAN = 'https://p3m.dev/cran/__linux__/noble/${CRAN_SNAPSHOT}')); \
    pkgs <- c('dplyr', 'tidyr', 'readr', 'stringr', 'lubridate', 'purrr', 'haven', 'xml2'); \
    install.packages(pkgs, Ncpus = 4); \
    stopifnot(all(vapply(pkgs, requireNamespace, logical(1), quietly = TRUE)))"

# --- Non-root agent user ----------------------------------------------------
RUN useradd --create-home --uid 1001 --shell /bin/bash agent \
    && mkdir -p /app/input /app/output /logs/agent /logs/verifier /logs/artifacts \
    && chown -R agent:agent /app /logs

# --- OpenCode offline settings: model catalog and plugin package ------------
# The run allows only the model API host, so OpenCode cannot fetch its model
# catalog (OPENCODE_MODELS_PATH) or its plugin package at startup.
RUN mkdir -p /opt/yamaa-eval \
    && curl -fsSL -o /opt/yamaa-eval/opencode-models.json https://models.dev/api.json \
    && python -c "import json, sys; d = json.load(open(sys.argv[1])); assert d['opencode-go']['models'] and d['anthropic']['models']" \
        /opt/yamaa-eval/opencode-models.json \
    && chmod 644 /opt/yamaa-eval/opencode-models.json
USER agent
RUN set -eux; \
    export HOME=/home/agent; \
    npm install --prefix "${HOME}/.config/opencode" --no-fund --no-audit \
        "@opencode-ai/plugin@${OPENCODE_VERSION}"; \
    npm cache clean --force
USER root

WORKDIR /app

LABEL org.opencontainers.image.title="yamaa-harbor-env" \
      org.opencontainers.image.source="https://github.com/elong0527/yamaa" \
      org.yamaa.harbor.r-version="${R_VERSION}" \
      org.yamaa.harbor.cran-snapshot="${CRAN_SNAPSHOT}" \
      org.yamaa.harbor.pypi-snapshot="${PYPI_SNAPSHOT}" \
      org.yamaa.harbor.opencode-version="${OPENCODE_VERSION}"
