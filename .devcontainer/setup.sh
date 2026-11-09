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

printf '\nInstalling opensas, OpenCode and Harbor...\n'
# The Harbor evaluation's installer pins the opensas release its images use.
case "$(uname -m)" in
  x86_64) opensas_architecture=amd64 ;;
  aarch64|arm64) opensas_architecture=arm64 ;;
  *) printf 'Unsupported opensas architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac
sudo bash evaluations/harbor/install-opensas.sh "$opensas_architecture"
npm install --global --prefix "$HOME/.local" @opencode/cli@2
# Harbor's version comes from the evaluation's own pin. It runs on CI's
# Python, 3.14: one of its dependencies does not build for 3.15 yet.
harbor_requirement="$(python - <<'PY'
import tomllib

with open("evaluations/harbor/pyproject.toml", "rb") as handle:
    dependencies = tomllib.load(handle)["project"]["dependencies"]
print(next(entry for entry in dependencies if entry.startswith("harbor==")))
PY
)"
uv tool install --force --python 3.14 "$harbor_requirement"

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

printf '\nInstalling the tidyverse and languageserver R packages...\n'
# pak installs the packages' system libraries through sudo. Rscript runs
# without --vanilla so that Rocker's site profile supplies its binary repository.
Rscript -e 'install.packages("pak", repos = sprintf("https://r-lib.github.io/p/pak/stable/%s/%s/%s", .Platform$pkgType, R.Version()$os, R.Version()$arch), lib = Sys.getenv("R_LIBS_USER"))'
PKG_SYSREQS=true Rscript -e 'pak::pkg_install(c("tidyverse", "languageserver"), lib = Sys.getenv("R_LIBS_USER"))'

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
for (package in c("tidyverse", "languageserver")) {
  library(package, character.only = TRUE)
  cat(package, format(packageVersion(package)), "\n")
}
RS
quarto --version
opencode --version
claude --version
codex --version
harbor --version
docker --version
printf 'data _null_; put "opensas ready"; run;\n' > "$scratch/smoke.sas"
opensas "$scratch/smoke.sas"
# The RTF preview extension converts with LibreOffice; prove that conversion.
printf '{\\rtf1\\ansi yamaa}\n' > "$scratch/smoke.rtf"
soffice --headless --convert-to pdf --outdir "$scratch" "$scratch/smoke.rtf"
test -s "$scratch/smoke.pdf"
printf '\nyamaa Codespace is ready. Run R, quarto, python, opensas, harbor, opencode, claude, or codex.\nPilot 7 data: %s\n' "$pilot7_dir"
