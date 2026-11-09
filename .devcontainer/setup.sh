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

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

printf '\nInstalling OpenCode...\n'
npm install --global --prefix "$HOME/.local" @opencode/cli@2

printf '\nInstalling Claude Code and Codex...\n'
npm install --global --prefix "$HOME/.local" @anthropic-ai/claude-code @openai/codex

printf '\nCloning the latest Pilot 7 synthetic data repository beside yamaa...\n'
pilot7_dir="$(dirname "$repository_root")/submissions-pilot7-synthetic-data"
if [ -d "$pilot7_dir/.git" ]; then
  git -C "$pilot7_dir" pull --ff-only ||
    printf 'warning: could not update %s; keeping the existing checkout\n' "$pilot7_dir" >&2
else
  git clone --depth 1 https://github.com/RConsortium/submissions-pilot7-synthetic-data "$pilot7_dir"
fi

# The editor opens the parent folder, so point its settings at this checkout.
mkdir -p "$(dirname "$repository_root")/.vscode"
cat > "$(dirname "$repository_root")/.vscode/settings.json" <<SETTINGS
{
  "python.defaultInterpreterPath": "$repository_root/python/.venv/bin/python",
  "rust-analyzer.linkedProjects": ["$repository_root/rust/Cargo.toml"],
  "terminal.integrated.cwd": "$repository_root"
}
SETTINGS

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
library(yamaa)
print(R.version.string)
report <- yamaa_selftest()
print(report)
stopifnot(report$status == "succeeded")
RS
quarto --version
opencode --version
claude --version
codex --version
printf '\nyamaa Codespace is ready. Run R, quarto, python, opencode, claude, or codex.\nPilot 7 data: %s\n' "$pilot7_dir"
