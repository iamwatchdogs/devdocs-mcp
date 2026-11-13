"""Eval regression gate: compare the fresh gate table against the tracked report.

Driven by `just eval-ci` (docs/implementation/14 §Regression gate). Every
gate row that is PASS in the committed tracked report must still be PASS in
the latest fresh run under eval/runs/. The accepted INCONCLUSIVE row
(Retrieval failures; its M4 baseline measurement is M11 work) never fails
the check, whichever way it reads.

Exit 0 = no regression, 1 = regression, 2 = structural error (a missing
report or run dir, an unparseable gate table).
"""

import sys
from pathlib import Path

STATUSES = {"PASS", "FAIL", "INCONCLUSIVE"}
DEFAULT_BASELINE = Path("eval/reports/2026-11-13-pilot/report.md")
RUNS = Path("eval/runs")


def gate_table(path):
    """Parse the `## Gate table` markdown rows into name -> (measured, threshold, status)."""
    rows = {}
    for line in path.read_text().splitlines():
        cells = [c.strip() for c in line.split("|")]
        # A gate row splits into 7 cells: ['', name, measured, threshold,
        # phase, status, '']. The header row's status cell reads 'Status'.
        if len(cells) == 7 and cells[5] in STATUSES and cells[1]:
            rows[cells[1]] = (cells[2], cells[3], cells[5])
    return rows


def main():
    baseline_path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_BASELINE
    if not baseline_path.is_file():
        print(f"error: baseline report missing: {baseline_path}", file=sys.stderr)
        return 2
    run_dirs = [p for p in RUNS.glob("*pilot*") if (p / "report.md").is_file()]
    if not run_dirs:
        print(f"error: no fresh run report under {RUNS}/ — run 'just eval' first", file=sys.stderr)
        return 2
    # Latest by mtime, not by name: the run dirs' -N suffix sorts
    # lexicographically wrong past -9 (pilot-9 > pilot-12).
    fresh_path = max(run_dirs, key=lambda p: (p / "report.md").stat().st_mtime) / "report.md"

    baseline = gate_table(baseline_path)
    fresh = gate_table(fresh_path)
    if not baseline or not fresh:
        print(f"error: gate table parse failed (baseline={bool(baseline)}, fresh={bool(fresh)})", file=sys.stderr)
        return 2

    print(f"baseline: {baseline_path}")
    print(f"fresh:    {fresh_path}")
    print()
    regressions = []
    for name, (measured, threshold, status) in baseline.items():
        row = fresh.get(name)
        if row is None:
            print(f"REGRESSION {name}: baseline {status} {measured}, fresh row missing")
            regressions.append(name)
            continue
        f_measured, f_threshold, f_status = row
        if status == "PASS" and f_status != "PASS":
            print(f"REGRESSION {name}: baseline {status} {measured} (threshold {threshold}),"
                  f" fresh {f_status} {f_measured}")
            regressions.append(name)
        else:
            print(f"ok         {name}: baseline {status} {measured} -> fresh {f_status} {f_measured}")
    print()
    if regressions:
        print(f"{len(regressions)} gate row(s) regressed against the tracked report:", file=sys.stderr)
        for name in regressions:
            print(f"  - {name}", file=sys.stderr)
        return 1
    print("no gate regression against the tracked report")
    return 0


if __name__ == "__main__":
    sys.exit(main())
