#!/usr/bin/env python3
import json
import os
import shutil
import statistics
import sys


def wall_times(result):
    if "measurements" in result:
        return [m["time_wall_clock"]["value"] for m in result["measurements"]]
    return list(result.get("times", []))


def main():
    out = sys.argv[1]
    rounds = [d for d in sys.argv[2:] if os.path.isdir(d)]
    names = set()
    for directory in rounds:
        names.update(f for f in os.listdir(directory) if f.endswith(".json"))
    for name in names:
        times = []
        for directory in rounds:
            path = os.path.join(directory, name)
            if not os.path.exists(path):
                continue
            try:
                with open(path) as f:
                    times += wall_times(json.load(f)["results"][0])
            except (ValueError, KeyError, IndexError):
                continue
        if not times:
            continue
        merged = {
            "mean": statistics.mean(times),
            "stddev": statistics.stdev(times) if len(times) > 1 else 0.0,
            "min": min(times),
            "times": times,
        }
        with open(os.path.join(out, name), "w") as f:
            json.dump({"results": [merged]}, f)
    if rounds:
        for name in ("pane-size", "version"):
            source = os.path.join(rounds[-1], name)
            if os.path.exists(source):
                shutil.copy(source, os.path.join(out, name))


if __name__ == "__main__":
    main()
