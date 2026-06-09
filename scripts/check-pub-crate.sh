#!/usr/bin/env bash
# check-pub-crate.sh — reject any active pub(crate) in Rust source files.
#
# This enforces Principle 7: No pub(crate). No exceptions.
# See CONTRIBUTING.md § Visibility Convention for the full rationale.
#
# Usage:
#   ./scripts/check-pub-crate.sh [search_dir] [violations_file]
#
#   search_dir      — directory to scan (default: src/)
#   violations_file — optional path; violations written here for CI consumption
#
# Exits 0 when no violations found, 1 otherwise.
set -euo pipefail

SEARCH_DIR="${1:-src}"
VIOLATIONS_FILE="${2:-}"
VIOLATIONS=0
VIOLATION_LIST=""

# Walk all .rs files under SEARCH_DIR.
while IFS= read -r file; do
    line_num=0
    while IFS= read -r line; do
        line_num=$((line_num + 1))

        # Strip comments: anything after // is ignored.
        code_part="${line%%//*}"

        # Skip if the pub(crate) only appears in a comment or string literal.
        # The strip above handles // comments; doc-tests and string literals that
        # contain pub(crate) are an acceptable false-negative — they are rare and
        # the reviewer can confirm manually.
        if echo "$code_part" | grep -qE '\bpub\s*\(\s*crate\s*\)'; then
            VIOLATIONS=$((VIOLATIONS + 1))
            entry="$file:$line_num: $line"
            VIOLATION_LIST="$VIOLATION_LIST
$entry"
            echo "VIOLATION: $entry" >&2
        fi
    done < "$file"
done < <(find "$SEARCH_DIR" -name "*.rs" -type f | sort)

if [ "$VIOLATIONS" -gt 0 ]; then
    echo "" >&2
    echo "Found $VIOLATIONS pub(crate) occurrence(s)." >&2
    echo "pub(crate) is banned by Principle 7. Convert each one to pub." >&2
    echo "See CONTRIBUTING.md for the full visibility convention." >&2

    if [ -n "$VIOLATIONS_FILE" ]; then
        echo "$VIOLATION_LIST" > "$VIOLATIONS_FILE"
    fi
    exit 1
fi

echo "pub-crate-guard: clean — no pub(crate) found."
exit 0
