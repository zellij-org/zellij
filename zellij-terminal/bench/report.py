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
    if "summary" in result:
        result = result["summary"]["time_wall_clock"]
    return result["mean"] * 1000, (result.get("stddev") or 0.0) * 1000


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
            text = "%.0f ± %.0f ms" % value
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


if __name__ == "__main__":
    main()
