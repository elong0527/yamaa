# Local checks and PR jobs use these same targets from the repository root.
.DEFAULT_GOAL := help
.PHONY: help setup quick check check-lint check-repository test-rust test-python \
        test-r docs-test docs-generate docs-build docs

UV ?= uv
PROJECT := $(UV) run --project python --no-sync
TOOLS := $(PROJECT) python -m

help:
	@python -m tools.commands.help

setup:
	python -m tools.commands.setup --uv "$(UV)"

# Before a push: lint without the R binding and the repository checks. Pull
# request CI runs the complete check, including every test suite.
quick:
	$(TOOLS) tools.commands.repository lint --without-r
	$(TOOLS) tools.commands.repository check

check: check-lint check-repository test-rust test-python test-r

check-lint:
	$(TOOLS) tools.commands.repository lint

check-repository:
	$(TOOLS) tools.commands.repository check

test-rust:
	$(TOOLS) tools.commands.rust_tests

test-python:
	$(TOOLS) tools.commands.python_tests

test-r:
	$(TOOLS) tools.commands.r_tests

docs-test:
	$(PROJECT) python -m unittest tools.tests.test_canonical_pages tools.tests.test_build_index tools.tests.test_generate tools.tests.test_mapping_doc

docs-generate:
	$(TOOLS) tools.commands.docs dashboards

docs-build:
	$(TOOLS) tools.commands.docs build

docs:
	$(TOOLS) tools.commands.docs dashboards
	$(TOOLS) tools.commands.docs build
