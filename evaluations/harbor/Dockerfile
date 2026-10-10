# syntax=docker/dockerfile:1.7
# Build the six named targets with: python evaluations/harbor/images.py build
# Each agent has one benchmark runtime; its verifier adds the Python grader.
ARG R_VERSION=4.6.1
ARG UV_VERSION=0.12.21

FROM ghcr.io/astral-sh/uv:${UV_VERSION} AS uv

FROM ubuntu:24.04 AS common
COPY image-common.sh /tmp/image-common.sh
RUN bash /tmp/image-common.sh && rm /tmp/image-common.sh
ENV LANG=en_US.UTF-8
WORKDIR /app

# OpenCode infrastructure is shared, without Python, R or opensas.
FROM common AS opencode
ARG OPENCODE_VERSION=1.18.33
ARG NVM_VERSION=v0.40.2
ARG NODE_MAJOR=22
USER agent
RUN bash -c 'set -eo pipefail; \
    export HOME=/home/agent NVM_DIR=/home/agent/.nvm; \
    curl -fsSL "https://raw.githubusercontent.com/nvm-sh/nvm/${NVM_VERSION}/install.sh" \
        | env -u NODE_VERSION bash; \
    . "${NVM_DIR}/nvm.sh"; \
    nvm install "${NODE_MAJOR}"; nvm alias default "${NODE_MAJOR}"; \
    npm i -g "opencode-ai@${OPENCODE_VERSION}"; \
    npm install --prefix "${HOME}/.config/opencode" --no-fund --no-audit \
        "@opencode-ai/plugin@${OPENCODE_VERSION}"; \
    npm cache clean --force; opencode --version; \
    mkdir -p "${HOME}/.local/bin"; \
    for program in node npm opencode; do \
        ln -s "$(command -v "${program}")" "${HOME}/.local/bin/${program}"; \
    done; \
    ln -s "$(command -v node)" "${HOME}/.local/bin/nodejs"'

FROM common AS python-runtime
ARG PYPI_SNAPSHOT=2026-09-22T00:00:00Z
COPY --from=uv /uv /uvx /usr/local/bin/
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-venv \
    && rm -rf /var/lib/apt/lists/* \
    && uv venv /opt/yamaa-eval/venv --python /usr/bin/python3 \
    && UV_EXCLUDE_NEWER="${PYPI_SNAPSHOT}" uv pip install \
        --python /opt/yamaa-eval/venv/bin/python numpy pandas polars pyarrow lxml
ENV PATH=/opt/yamaa-eval/venv/bin:${PATH} UV_NO_CACHE=1

FROM rocker/r-ver:${R_VERSION} AS r-runtime
ARG R_VERSION=4.6.1
ARG CRAN_SNAPSHOT=2026-09-27
COPY image-common.sh /tmp/image-common.sh
RUN bash /tmp/image-common.sh && rm /tmp/image-common.sh \
    && apt-get update && apt-get install -y --no-install-recommends \
        libxml2-dev zlib1g-dev libicu-dev \
    && rm -rf /var/lib/apt/lists/*
RUN R -q -e "options(repos = c(CRAN = 'https://p3m.dev/cran/__linux__/noble/${CRAN_SNAPSHOT}')); \
    pkgs <- c('dplyr', 'tidyr', 'readr', 'stringr', 'lubridate', 'purrr', 'haven', 'xml2', 'nanoparquet'); \
    install.packages(pkgs, Ncpus = 4); \
    stopifnot(all(vapply(pkgs, requireNamespace, logical(1), quietly = TRUE)))"
ENV LANG=en_US.UTF-8
WORKDIR /app

FROM common AS opensas-runtime
ARG TARGETARCH
COPY install-opensas.sh /tmp/install-opensas.sh
COPY licenses/opensas/ /usr/local/share/licenses/opensas/
RUN bash /tmp/install-opensas.sh "${TARGETARCH}" && rm /tmp/install-opensas.sh
LABEL org.yamaa.harbor.opensas-version="0.6.6" \
      org.yamaa.harbor.opensas-source="1e8bc655b6a56c58ab20e263e7a2638063834768"

FROM python-runtime AS agent-python
COPY --from=opencode --chown=agent:agent /home/agent/ /home/agent/
ENV OPENCODE_DISABLE_MODELS_FETCH=1 PATH=/home/agent/.local/bin:${PATH}
LABEL org.yamaa.harbor.language="python" org.yamaa.harbor.role="agent"

FROM r-runtime AS agent-r
COPY --from=opencode --chown=agent:agent /home/agent/ /home/agent/
ENV OPENCODE_DISABLE_MODELS_FETCH=1 PATH=/home/agent/.local/bin:${PATH}
LABEL org.yamaa.harbor.language="r" org.yamaa.harbor.role="agent"

FROM opensas-runtime AS agent-opensas
COPY --from=opencode --chown=agent:agent /home/agent/ /home/agent/
ENV OPENCODE_DISABLE_MODELS_FETCH=1 PATH=/home/agent/.local/bin:${PATH}
LABEL org.yamaa.harbor.language="opensas" org.yamaa.harbor.role="agent"

FROM python-runtime AS verifier-python
LABEL org.yamaa.harbor.language="python" org.yamaa.harbor.role="verifier"

FROM r-runtime AS verifier-r
ARG PYPI_SNAPSHOT=2026-09-22T00:00:00Z
COPY --from=uv /uv /usr/local/bin/
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-venv \
    && rm -rf /var/lib/apt/lists/* \
    && uv venv /opt/yamaa-eval/venv --python /usr/bin/python3 \
    && UV_EXCLUDE_NEWER="${PYPI_SNAPSHOT}" uv pip install \
        --python /opt/yamaa-eval/venv/bin/python pyarrow
ENV PATH=/opt/yamaa-eval/venv/bin:${PATH} UV_NO_CACHE=1
LABEL org.yamaa.harbor.language="r" org.yamaa.harbor.role="verifier"

FROM opensas-runtime AS verifier-opensas
RUN apt-get update && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
LABEL org.yamaa.harbor.language="opensas" org.yamaa.harbor.role="verifier"
