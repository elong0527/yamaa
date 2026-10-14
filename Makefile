# Supported local entry points. CI calls the same Python scripts directly so it
# does not need GNU make on Windows. Run from the repository root.
.DEFAULT_GOAL := help
.PHONY: help sync-python check-fast check-repository check-blockers test-rust \
        stage-python test-python stage-r stage-r-tests test-r check-r \
        docs-test docs-generate docs-build docs audit-native audit-replay

PYTHON ?= python
DOCS_PYTHON ?= $(PYTHON)
HOST_PYTHON ?= $(PYTHON)
HOST_WORK ?=
OUTPUT ?=
SOURCE_REVISION ?= $(shell git rev-parse HEAD)
PACKAGE_FORM ?= source
ARTIFACT_REFERENCE ?=
HOST_ARTIFACT_REFERENCE ?=
EVIDENCE ?=
R_STAGE ?=
R_ARCHIVE ?=
RUN_ID ?=
AUDIT_INPUT ?=
REPOSITORY ?= elong0527/yamaa
COMMANDS := .github/scripts/commands

help:
	@$(PYTHON) $(COMMANDS)/help.py

sync-python:
	uv sync --project python --extra test --locked

check-fast:
	$(PYTHON) $(COMMANDS)/repository.py fast

check-repository:
	$(PYTHON) $(COMMANDS)/repository.py repository

check-blockers:
	$(PYTHON) $(COMMANDS)/repository.py blockers

test-rust:
	$(PYTHON) $(COMMANDS)/rust_tests.py

stage-python:
	$(PYTHON) $(COMMANDS)/installed.py stage --runtime python --staged-root "$(HOST_WORK)"

test-python:
	$(HOST_PYTHON) -I $(COMMANDS)/installed.py run --runtime python \
	  --staged-root "$(HOST_WORK)" --output "$(OUTPUT)" --source-revision "$(SOURCE_REVISION)" \
	  --package-form "$(PACKAGE_FORM)" --artifact-reference "$(ARTIFACT_REFERENCE)" \
	  --host-artifact-reference "$(HOST_ARTIFACT_REFERENCE)" --evidence "$(EVIDENCE)"

stage-r:
	@test -n "$(R_STAGE)" || (echo 'Set R_STAGE to a fresh package directory outside the checkout' >&2; exit 2)
	$(PYTHON) rust/tools/stage_r_package.py "$(R_STAGE)"

stage-r-tests:
	$(PYTHON) $(COMMANDS)/installed.py stage --runtime r --staged-root "$(HOST_WORK)"

test-r:
	$(PYTHON) $(COMMANDS)/installed.py run --runtime r \
	  --staged-root "$(HOST_WORK)" --output "$(OUTPUT)" --source-revision "$(SOURCE_REVISION)" \
	  --package-form source --artifact-reference "$(ARTIFACT_REFERENCE)" --evidence "$(EVIDENCE)"

check-r:
	$(PYTHON) $(COMMANDS)/check_r.py --archive "$(R_ARCHIVE)" --work-dir "$(HOST_WORK)"

docs-test:
	$(DOCS_PYTHON) $(COMMANDS)/docs.py test

docs-generate:
	$(DOCS_PYTHON) $(COMMANDS)/docs.py dashboards

docs-build:
	$(DOCS_PYTHON) $(COMMANDS)/docs.py build

docs: docs-generate
	$(DOCS_PYTHON) $(COMMANDS)/docs.py build

audit-native:
	@test -n "$(RUN_ID)" -a -n "$(OUTPUT)" || (echo 'Set RUN_ID and fresh OUTPUT' >&2; exit 2)
	$(PYTHON) rust/tools/audit_native_run.py --run-id "$(RUN_ID)" --repository "$(REPOSITORY)" --output "$(OUTPUT)" --check

audit-replay:
	@test -n "$(AUDIT_INPUT)" -a -n "$(OUTPUT)" || (echo 'Set retained AUDIT_INPUT and fresh OUTPUT' >&2; exit 2)
	$(PYTHON) rust/tools/audit_native_run.py --input "$(AUDIT_INPUT)" --output "$(OUTPUT)" --check
