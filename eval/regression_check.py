"""Eval regression gate: compare the fresh gate table against the tracked report.

Driven by `just eval-ci` (docs/implementation/14 §Regression gate). Every
gate row that is PASS in the committed tracked report must still be PASS in
the fresh run report — passed explicitly (eval-ci hands over the run dir
its `just eval` call reported) or, by fallback, the newest one under
eval/runs/. The accepted INCONCLUSIVE row (Retrieval failures; its M4
baseline measurement is M11 work) never fails the check, whichever way it
reads.

Usage: regression_check.py [baseline-report] [--fresh fresh-report]

Exit 0 = no regression, 1 = regression, 2 = structural error (a missing
report or run dir, an unparseable gate table, a gate set that is not
exactly the six expected names).
"""

import sys
from pathlib import Path

STATUSES = {"PASS", "FAIL", "INCONCLUSIVE"}
# The six gate names the tracked report's table carries. A table missing
# one (or carrying a renamed one) must fail structurally, not silently
# compare a subset.
EXPECTED_GATES = {
    "Symbol recall",
    "hit@5",
    "p95 latency",
    "Cost per 1,000 queries",
    "Retrieval failures",
    "Version correctness",
}
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


def parse_fresh_arg(argv):
    """Split the `--fresh <report>` pair off the argument list. Returns
    (fresh report or None, remaining args), or (None, None) after printing
    the usage error."""
    if "--fresh" not in argv:
        return None, argv
    i = argv.index("--fresh")
    if i + 1 >= len(argv):
        print("error: --fresh needs a fresh report path", file=sys.stderr)
        return None, None
    return argv[i + 1], argv[:i] + argv[i + 2:]


def resolve_fresh_path(fresh_arg):
    """The fresh report path: the `--fresh` handover, else the newest run
    report under eval/runs/. Returns None after printing the structural
    error."""
    if fresh_arg:
        # The fresh report this eval-ci run actually produced. Two concurrent
        # `just eval-ci` calls in one checkout would otherwise race the
        # newest-glob fallback below and compare the other run's report.
        return Path(fresh_arg)
    run_dirs = [p for p in RUNS.glob("*pilot*") if (p / "report.md").is_file()]
    if not run_dirs:
        print(f"error: no fresh run report under {RUNS}/ — run 'just eval' first", file=sys.stderr)
        return None
    # Latest by mtime, not by name: the run dirs' -N suffix sorts
    # lexicographically wrong past -9 (pilot-9 > pilot-12).
    return max(run_dirs, key=lambda p: (p / "report.md").stat().st_mtime) / "report.md"


def find_regressions(baseline, fresh):
    """Compare the tables row by row and print one line per row. Returns
    the regressed gate names in baseline row order."""
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
    return regressions


def report_verdict(baseline_path, fresh_path):
    """Parse both gate tables, validate the gate set, run the comparison,
    and print the verdict. Returns 0 with no regression, 1 with one, or 2
    on a structural error."""
    baseline = gate_table(baseline_path)
    fresh = gate_table(fresh_path)
    if not baseline or not fresh:
        print(f"error: gate table parse failed (baseline={bool(baseline)}, fresh={bool(fresh)})", file=sys.stderr)
        return 2
    for label, table in (("baseline", baseline), ("fresh", fresh)):
        missing = EXPECTED_GATES - set(table)
        extra = set(table) - EXPECTED_GATES
        if missing or extra:
            print(f"error: {label} gate table does not carry exactly the expected gates"
                  f" (missing={sorted(missing)}, unexpected={sorted(extra)})", file=sys.stderr)
            return 2

    print(f"baseline: {baseline_path}")
    print(f"fresh:    {fresh_path}")
    print()
    regressions = find_regressions(baseline, fresh)
    print()
    if regressions:
        print(f"{len(regressions)} gate row(s) regressed against the tracked report:", file=sys.stderr)
        for name in regressions:
            print(f"  - {name}", file=sys.stderr)
        return 1
    print("no gate regression against the tracked report")
    return 0


def main():
    fresh_arg, rest = parse_fresh_arg(sys.argv[1:])
    if rest is None:
        return 2
    baseline_path = Path(rest[0]) if rest else DEFAULT_BASELINE
    if not baseline_path.is_file():
        print(f"error: baseline report missing: {baseline_path}", file=sys.stderr)
        return 2
    fresh_path = resolve_fresh_path(fresh_arg)
    if fresh_path is None:
        return 2
    if not fresh_path.is_file():
        print(f"error: fresh report missing: {fresh_path}", file=sys.stderr)
        return 2
    return report_verdict(baseline_path, fresh_path)


if __name__ == "__main__":
    sys.exit(main())
