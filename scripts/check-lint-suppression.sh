#!/usr/bin/env bash
set -euo pipefail

# Checks that every #[allow(...)], #![allow(...)], #[expect(...)], and
# #![expect(...)] attribute in Rust source files is preceded by a comment
# explaining *why* the suppression is necessary.
#
# Usage: check-lint-suppression.sh [search_dir] [violations_file]
#   search_dir      — directory to scan (default: .)
#   violations_file — optional path; violations are written here for CI consumption
#
# Exits 0 when all suppressions are justified, 1 when violations are found.

SEARCH_DIR="${1:-.}"
VIOLATIONS_FILE="${2:-}"
VIOLATIONS=0
VIOLATION_LIST=""

while IFS= read -r file; do
    line_num=0
    prev_line=""
    prev_prev_line=""

    while IFS= read -r line; do
        line_num=$((line_num + 1))
        trimmed="${line#"${line%%[![:space:]]*}"}"

        if echo "$trimmed" | grep -qE '^#!?\[(allow|expect)\('; then
            has_comment=false

            check="${prev_line#"${prev_line%%[![:space:]]*}"}"
            if echo "$check" | grep -qE '^//'; then
                has_comment=true
            fi

            if [ "$has_comment" = false ]; then
                check2="${prev_prev_line#"${prev_prev_line%%[![:space:]]*}"}"
                if echo "$check2" | grep -qE '^//'; then
                    has_comment=true
                fi
            fi

            if [ "$has_comment" = false ]; then
                if echo "$line" | grep -qE '//'; then
                    has_comment=true
                fi
            fi

            if [ "$has_comment" = false ]; then
                VIOLATIONS=$((VIOLATIONS + 1))
                VIOLATION_LIST="${VIOLATION_LIST}- \`${file}:${line_num}\`: \`${trimmed}\`\n"
            fi
        fi

        prev_prev_line="$prev_line"
        prev_line="$line"
    done < "$file"
done < <(find "$SEARCH_DIR" -name '*.rs' -not -path '*/target/*')

if [ "$VIOLATIONS" -gt 0 ]; then
    echo "===================================================="
    echo " LINT-SUPPRESSION GUARD: ${VIOLATIONS} violation(s) found"
    echo "===================================================="
    echo ""
    echo "Every #[allow(...)] / #[expect(...)] attribute must have a"
    echo "comment on the line above (or inline) explaining WHY the"
    echo "suppression is needed."
    echo ""
    echo "Violations:"
    printf '%b' "$VIOLATION_LIST"
    echo ""
    echo "Example of a valid suppression:"
    echo ""
    echo "  // Bevy queries require many generic params; a type alias"
    echo "  // would obscure which components the system accesses."
    echo '  #[allow(clippy::type_complexity)]'
    echo ""

    if [ -n "$VIOLATIONS_FILE" ]; then
        printf '%b' "$VIOLATION_LIST" > "$VIOLATIONS_FILE"
    fi

    exit 1
fi

echo "Lint-suppression guard passed — all suppressions have comments."

if [ -n "$VIOLATIONS_FILE" ]; then
    : > "$VIOLATIONS_FILE"
fi

exit 0
