#!/usr/bin/env python3
"""Fail CI when player-facing code violates GDD design principles.

Existing matches live in scripts/design-principles/baseline.txt (a ratchet).
New matches fail. Stale baseline rows fail. The list only shrinks.

Usage:
  python3 scripts/check-design-principles.py
  python3 scripts/check-design-principles.py --update-baseline
  python3 scripts/check-design-principles.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RULES_PATH = ROOT / "scripts" / "design-principles" / "rules.json"
BASELINE_PATH = ROOT / "scripts" / "design-principles" / "baseline.txt"


def load_rules(path: Path) -> dict:
    data = json.loads(path.read_text(encoding="utf-8"))
    if "rules" not in data:
        raise SystemExit(f"{path}: missing 'rules'")
    return data


def is_excluded(rel: str, path_exclude: list[str]) -> bool:
    return any(re.search(pat, rel) for pat in path_exclude)


def rust_files(
    repo_root: Path, scan_roots: list[str], path_exclude: list[str]
) -> list[Path]:
    files: list[Path] = []
    for root_name in scan_roots:
        root = repo_root / root_name
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.rs")):
            rel = path.relative_to(repo_root).as_posix()
            if is_excluded(rel, path_exclude):
                continue
            files.append(path)
    return files


def allow_ids_from(line: str, prefix: str) -> set[str]:
    if prefix not in line:
        return set()
    _, _, rest = line.partition(prefix)
    rest = rest.strip()
    ident = rest.split()[0] if rest else ""
    ident = ident.rstrip(".,;:—-")
    return {ident} if ident else set()


def scan(
    repo_root: Path, rules_data: dict, files: list[Path]
) -> list[tuple[str, int, str, str]]:
    """Return (relpath, line_no, rule_id, snippet) matches."""
    prefix = rules_data["allow_comment_prefix"]
    compiled = [
        (rule["id"], re.compile(rule["line_regex"])) for rule in rules_data["rules"]
    ]
    hits: list[tuple[str, int, str, str]] = []
    for path in files:
        rel = path.relative_to(repo_root).as_posix()
        lines = path.read_text(encoding="utf-8").splitlines()
        prev_allows: set[str] = set()
        for i, line in enumerate(lines, start=1):
            stripped = line.strip()
            line_allows = allow_ids_from(line, prefix)
            allowed = prev_allows | line_allows
            prev_allows = line_allows
            # Comments and rustdoc are not player-facing copy.
            if stripped.startswith("//"):
                continue
            for rule_id, regex in compiled:
                if rule_id in allowed:
                    continue
                if regex.search(line):
                    snippet = stripped
                    if len(snippet) > 160:
                        snippet = snippet[:157] + "..."
                    hits.append((rel, i, rule_id, snippet))
    return hits


def load_baseline(path: Path) -> set[tuple[str, int, str]]:
    if not path.exists():
        return set()
    rows: set[tuple[str, int, str]] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(":")
        if len(parts) < 3:
            raise SystemExit(f"malformed baseline row: {raw!r}")
        rel, line_s, rule_id = parts[0], parts[1], parts[2]
        rows.add((rel, int(line_s), rule_id))
    return rows


def format_baseline(hits: list[tuple[str, int, str, str]]) -> str:
    header = (
        "# Design-principle baseline — known existing violations.\n"
        "# Do not add rows in feature PRs. Shrink this file when gameplay fixes land.\n"
        "# format: path:line:rule_id\n"
    )
    rows = [f"{rel}:{lineno}:{rule_id}" for rel, lineno, rule_id, _ in hits]
    rows.sort()
    return header + "\n".join(rows) + ("\n" if rows else "")


def report(
    hits: list[tuple[str, int, str, str]],
    baseline: set[tuple[str, int, str]],
    rules_data: dict,
) -> int:
    hit_keys = {(rel, lineno, rule_id) for rel, lineno, rule_id, _ in hits}
    new_hits = [h for h in hits if (h[0], h[1], h[2]) not in baseline]
    stale = sorted(baseline - hit_keys)

    by_id = {rule["id"]: rule for rule in rules_data["rules"]}

    if not new_hits and not stale:
        baselined = len(hits)
        print(
            f"Design-principle check passed "
            f"({baselined} known violation(s) in baseline, 0 new)."
        )
        if baselined:
            print(
                "Baseline shrinks when those lines are fixed. "
                "Do not add new rows to scripts/design-principles/baseline.txt."
            )
        return 0

    print("=" * 60)
    print(" DESIGN PRINCIPLE GATE")
    print("=" * 60)
    print()
    print("Player-facing code must not confirm, list empty ??? slots,")
    print("show observation counts, or fake diegetic UI with zero-range HUDs.")
    print("See docs/bmad/epics/25-design-principle-ci.md")
    print()

    if new_hits:
        print(f"New violations ({len(new_hits)}) — not in baseline:")
        print()
        for rel, lineno, rule_id, snippet in new_hits:
            rule = by_id.get(rule_id, {})
            print(f"  {rel}:{lineno}  [{rule_id}]")
            print(f"    test:  {rule.get('design_test', '?')}")
            print(f"    rule:  {rule.get('principle', '')}")
            print(f"    line:  {snippet}")
            print()
        print("Fix the feature. Do not add these rows to the baseline.")
        print(
            "Urgent exception: // design-principle-allow: "
            f"{new_hits[0][2]} — <reason> on the line above."
        )
        print()

    if stale:
        print(f"Stale baseline rows ({len(stale)}) — no longer match:")
        print()
        for rel, lineno, rule_id in stale:
            print(f"  {rel}:{lineno}:{rule_id}")
        print()
        print("Remove them from scripts/design-principles/baseline.txt")
        print("(or run: python3 scripts/check-design-principles.py --update-baseline)")
        print()

    return 1


def self_test() -> int:
    """Tiny fixture test so the gate has a unit check of its own."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        src = tmp_path / "src"
        src.mkdir()
        (src / "ok.rs").write_text('let x = "Reliably holds";\n', encoding="utf-8")
        (src / "bad.rs").write_text(
            'let label = "Confirmed";\n'
            "// design-principle-allow: DP-CONFIRM-CONFIRMED — fixture\n"
            'let allowed = "Confirmed";\n'
            '// comment mentioning Confirmed should be ignored\n',
            encoding="utf-8",
        )
        (src / "bad_tests.rs").write_text(
            'assert_eq!(x, "Confirmed");\n', encoding="utf-8"
        )

        rules = {
            "scan_roots": ["src"],
            "path_exclude": ["_tests\\.rs$"],
            "allow_comment_prefix": "design-principle-allow:",
            "rules": [
                {
                    "id": "DP-CONFIRM-CONFIRMED",
                    "design_test": "Confirmation",
                    "principle": "never confirm",
                    "line_regex": "Confirmed",
                }
            ],
        }

        files = rust_files(tmp_path, rules["scan_roots"], rules["path_exclude"])
        hits = scan(tmp_path, rules, files)

        keys = {(rel, rule_id) for rel, _, rule_id, _ in hits}
        if keys != {("src/bad.rs", "DP-CONFIRM-CONFIRMED")}:
            print(
                "self-test failed: expected only src/bad.rs:DP-CONFIRM-CONFIRMED",
                keys,
                hits,
            )
            return 1
        if any(h[1] != 1 for h in hits):
            print("self-test failed: allow comment should suppress line 3", hits)
            return 1
        print("self-test passed")
        return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Rewrite baseline.txt from current matches. For shrinking after fixes, not for new drift.",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    rules_data = load_rules(RULES_PATH)
    files = rust_files(ROOT, rules_data["scan_roots"], rules_data["path_exclude"])
    hits = scan(ROOT, rules_data, files)

    if args.update_baseline:
        BASELINE_PATH.write_text(format_baseline(hits), encoding="utf-8")
        print(f"Wrote {len(hits)} baseline row(s) to {BASELINE_PATH.relative_to(ROOT)}")
        return 0

    baseline = load_baseline(BASELINE_PATH)
    return report(hits, baseline, rules_data)


if __name__ == "__main__":
    sys.exit(main())
