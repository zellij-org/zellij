#!/usr/bin/env python3
import json
import os
import sys

BASE_CASES = ["ascii", "log", "color", "unicode"]


def load(directory, name):
    path = os.path.join(directory, name + ".json")
    if not os.path.exists(path):
        return None
    try:
        with open(path) as f:
            result = json.load(f)["results"][0]
    except (ValueError, KeyError, IndexError):
        return None
    if "measurements" in result:
        count = len(result["measurements"])
    else:
        count = len(result.get("times", [])) or 1
    if "summary" in result:
        result = result["summary"]["time_wall_clock"]
    return result["mean"] * 1000, (result.get("stddev") or 0.0) * 1000, count


def read_text(directory, name):
    path = os.path.join(directory, name)
    if not os.path.exists(path):
        return ""
    with open(path) as f:
        return f.read().strip()


def cases_in(dirs):
    found = []
    for directory in dirs:
        for name in sorted(os.listdir(directory)):
            if name.endswith(".json"):
                case = name[: -len(".json")].replace("+processed", "")
                if case not in found:
                    found.append(case)
    return [c for c in BASE_CASES if c in found] + [c for c in found if c not in BASE_CASES]


def file_sizes(cases):
    fixtures = os.environ.get("REPORT_FIXTURES", "")
    sizes = {}
    for case in cases:
        path = os.path.join(fixtures, case + ".txt")
        if fixtures and os.path.exists(path):
            sizes[case] = os.path.getsize(path)
    return sizes


def table(title, labels, dirs, cases, suffix, sizes):
    rows = []
    for case in cases:
        values = [load(d, case + suffix) for d in dirs]
        means = [v[0] for v in values if v]
        best = min(means) if means else None
        cells = []
        for value in values:
            if not value:
                cells.append("-")
                continue
            text = "%.0f ± %.0f ms" % value[:2]
            if case in sizes and value[0] > 0:
                text += " (%.0f MB/s)" % (sizes[case] / 1e6 / (value[0] / 1000))
            if len(means) > 1 and value[0] == best:
                text = "**" + text + "**"
            cells.append(text)
        rows.append([case] + cells)
    header = ["file"] + labels
    widths = [max(len(r[i]) for r in rows + [header]) for i in range(len(header))]

    def line(cells):
        return "| " + " | ".join(c.ljust(w) for c, w in zip(cells, widths)) + " |"

    out = ["### " + title, "", line(header), "|" + "|".join("-" * (w + 2) for w in widths) + "|"]
    out += [line(r) for r in rows]
    return "\n".join(out)


def difference_table(title, labels, dirs, cases, suffix, base):
    base_dir = dirs[labels.index(base)]
    others = [(l, d) for l, d in zip(labels, dirs) if l != base and l.startswith("no-")]
    if not others:
        return ""
    header = ["file"] + [l for l, _ in others]
    rows = []
    for case in cases:
        reference = load(base_dir, case + suffix)
        row = [case]
        for _, directory in others:
            value = load(directory, case + suffix)
            if not reference or not value:
                row.append("-")
                continue
            change = (value[0] - reference[0]) / reference[0] * 100
            noise = (value[1] ** 2 / value[2] + reference[1] ** 2 / reference[2]) ** 0.5
            significant = abs(value[0] - reference[0]) > 2 * noise
            row.append("%+.0f%%%s" % (change, "" if significant else " ~"))
        rows.append(row)
    widths = [max(len(r[i]) for r in rows + [header]) for i in range(len(header))]

    def line(cells):
        return "| " + " | ".join(c.ljust(w) for c, w in zip(cells, widths)) + " |"

    out = ["### " + title, "", line(header), "|" + "|".join("-" * (w + 2) for w in widths) + "|"]
    out += [line(r) for r in rows]
    out += [
        "",
        "Time with that group turned off, compared with %s with everything on. "
        "Positive means slower without it, so the group helps; \"~\" means the "
        "difference is within the noise." % base,
    ]
    return "\n".join(out)


def main():
    labels = sys.argv[1::2]
    dirs = sys.argv[2::2]
    expect = os.environ.get("EXPECT", "")
    meta = os.environ.get("REPORT_META", "")
    cases = cases_in(dirs)
    sizes = file_sizes(cases)

    print("## Terminal throughput")
    print()
    if meta and os.path.exists(meta):
        with open(meta) as f:
            for line in f:
                if line.strip():
                    print("- " + line.strip())
    for label, directory in zip(labels, dirs):
        size = read_text(directory, "pane-size")
        version = read_text(directory, "version")
        text = "- %s" % label
        if version:
            text += ": %s" % version
        if size:
            text += ", pane %s" % size.replace("x", " x ")
            if expect and size != expect:
                text += " (**size mismatch**, expected %s)" % expect.replace("x", " x ")
        print(text)
    print()
    print(table("Until processed (compare these)", labels, dirs, cases, "+processed", sizes))
    print()
    print(table("Until cat exits", labels, dirs, cases, "", sizes))
    print()
    print("Mean ± standard deviation; lower is better; the fastest in each row is in bold.")
    base = os.environ.get("REPORT_BASE", "")
    if base and base in labels:
        print()
        print(difference_table("Difference from zellij (until processed)", labels, dirs, cases, "+processed", base))


if __name__ == "__main__":
    main()
