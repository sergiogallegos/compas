$ErrorActionPreference = "Stop"

git config core.hooksPath .githooks
Write-Host "Git hooks installed: core.hooksPath=.githooks"
Write-Host "The pre-commit hook runs fmt, clippy, version consistency, and frontend typecheck when node_modules exists."
