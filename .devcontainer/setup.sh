#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
repository_root="$PWD"
export PATH="$repository_root/python/.venv/bin:$HOME/.local/bin:$PATH"
export R_LIBS_USER="${R_LIBS_USER:-$HOME/R/library}"
mkdir -p "$HOME/.local/bin" "$R_LIBS_USER"

# Codespaces supplies the complete repository checkout as the workspace.
# Resolve the latest stable CPython, rather than the base image's system Python.
uv python install --default --force 3
uv venv --clear --seed --python "$HOME/.local/bin/python3" python/.venv
export UV_PYTHON="$repository_root/python/.venv/bin/python"

printf '\nInstalling opensas and OpenCode...\n'
case "$(uname -m)" in
  x86_64) opensas_asset=opensas-amd ;;
  aarch64|arm64) opensas_asset=opensas-arm ;;
  *) printf 'Unsupported opensas architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
curl --fail --location --retry 3 \
  "https://github.com/kirha-ai/opensas/releases/latest/download/$opensas_asset" \
  --output "$scratch/opensas"
install -m 0755 "$scratch/opensas" "$HOME/.local/bin/opensas"
npm install --global --prefix "$HOME/.local" opencode-ai@latest

printf '\nBuilding the Python package and installing repository tools...\n'
# The owned setup command installs yamaa and Polars along with the locked
# development tools, using the Rust toolchain declared by the checkout.
make setup
RUSTUP_TOOLCHAIN="$(python -c 'from tools.ci.toolchain import channel; print(channel())')"
export RUSTUP_TOOLCHAIN

printf '\nBuilding the R package from the same checkout...\n'
# R/yamaa alone is not installable: stage its shared Rust sources and vendor
# bundle through the repository's packaging command first.
python -m tools.staging.stage_r_package "$scratch/yamaa"
R CMD INSTALL --library="$R_LIBS_USER" "$scratch/yamaa"

printf '\nChecking the installed environment...\n'
python --version
python - <<'PY'
import polars
import yamaa

print(f"Polars {polars.__version__}")
report = yamaa.selftest()
print(report)
assert report.status == "succeeded", "Python yamaa selftest failed"
PY
Rscript --vanilla - <<'RS'
library(tidyverse)
library(yamaa)
print(R.version.string)
report <- yamaa_selftest()
print(report)
stopifnot(report$status == "succeeded")
RS
printf 'data _null_; put "opensas ready"; run;\n' > "$scratch/smoke.sas"
opensas "$scratch/smoke.sas"
opencode --version
printf '\nyamaa Codespace is ready. Run R, python, opensas, or opencode.\n'
