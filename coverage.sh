#!/usr/bin/env bash
# coverage.sh — Run code coverage for agentic-repos
#
# Usage:
#   ./coverage.sh              # Full coverage report (terminal + HTML)
#   ./coverage.sh --ci         # CI mode: generate lcov + summary only
#   ./coverage.sh --quick      # Quick: lib tests only (no integration)
#
# Output:
#   target/tarpaulin/           — HTML report
#   target/tarpaulin/lcov.info  — lcov format for CI
#   /tmp/coverage-agentic-repos/ — if running from outside

set -euo pipefail

cd "$(dirname "$0")"

MODE="${1:-full}"
OUTPUT_DIR="target/tarpaulin"

mkdir -p "$OUTPUT_DIR"

echo "🧪 Running code coverage for agentic-repos..."
echo "   Mode: $MODE"

case "$MODE" in
  --ci)
    nix develop --command cargo tarpaulin \
      --lib \
      --timeout 600 \
      --out Lcov \
      --out Stdout \
      --output-dir "$OUTPUT_DIR" \
      --skip-clean
    echo "✅ Coverage report generated in $OUTPUT_DIR/"
    echo "   lcov: $OUTPUT_DIR/lcov.info"
    ;;

  --quick)
    nix develop --command cargo tarpaulin \
      --lib \
      --timeout 300 \
      --out Stdout \
      --out Html \
      --output-dir "$OUTPUT_DIR"
    echo "✅ Quick coverage report in $OUTPUT_DIR/"
    ;;

  *)
    nix develop --command cargo tarpaulin \
      --lib \
      --timeout 600 \
      --out Stdout \
      --out Html \
      --out Lcov \
      --output-dir "$OUTPUT_DIR"
    echo "✅ Full coverage report in $OUTPUT_DIR/"
    echo "   Open: $OUTPUT_DIR/index.html"
    ;;
esac
