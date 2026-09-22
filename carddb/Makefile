# `=`  lazy assignment, `:=` immediate assignment, `?=` default that the caller can override
BOLD = \033[1m
GREEN = \033[0;32m
NC = \033[0m

lint:
	@ruff check --select I --fix
	@ruff format
	@ruff check
	@mypy
	@echo "\n$(GREEN)$(BOLD)🎉 All checks passed! 🎉$(NC)"

fix:
	@ruff check --fix --unsafe-fixes

noqa:
	@ruff check --ignore-noqa

toml-fmt:
	@taplo format -o array_auto_collapse=false -o indent_string="    "


clear-pyc:
	@find . -type d -name "__pycache__" -exec rm -r {} +
	@find . -type d -empty -delete
	@echo "$(GREEN)$(BOLD)All __pycache__ removed!$(NC)"

clear-cache:
	@rm -rf .ruff_cache
	@rm -rf .mypy_cache
	@rm -rf .pytest_cache
	@echo "$(GREEN)$(BOLD)All cache removed!$(NC)"

clear-cov:
	@rm -rf htmlcov
	@echo "$(GREEN)$(BOLD)Coverage report removed!$(NC)"

clear-all: clear-pyc clear-cache clear-cov


lock-update:
	uv lock --upgrade

install:
	uv sync --all-groups


help:
	@echo "lint - run ruff check and ruff format and mypy"
	@echo "fix - run ruff check with --fix and --unsafe-fix"
	@echo "noqa - run ruff check with --ignore-noqa"
	@echo "toml-fmt - format toml file"

	@echo "clear-pyc - remove all __pycache__ directories"
	@echo "clear-cache - remove .ruff_cache, .mypy_cache, .pytest_cache"
	@echo "clear-cov - remove coverage report"
	@echo "clear-all - remove all __pycache__, linter and test cache, htmlcov"

	@echo "lock-update - update lock file"
	@echo "install - install all dependencies"

	@echo "help - show this help message"
