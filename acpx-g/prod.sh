#!/bin/bash
set -e

cd "$(dirname "$0")"

# Defaults
export PORT="${PORT:-3000}"
export DATABASE_URL="${DATABASE_URL:-sqlite:acpx-g.db?mode=rwc}"
export RUST_LOG="${RUST_LOG:-acpx_g=debug,info}"

echo "=== acpx-g dev server ==="
echo "  http://0.0.0.0:${PORT}"
echo "  db: ${DATABASE_URL}"
echo

acpx-g --workflow-dir ./examples/
