#!/usr/bin/env python3
import os
import random
import sys

SIZE = int(os.environ.get("BENCH_SIZE", str(8 * 1024 * 1024)))

WORDS = (
    "the quick brown fox jumps over lazy dog terminal output render frame "
    "parse buffer scroll cursor session pane window client server thread "
    "error warning info debug request response latency throughput value"
).split()

CJK = "漢字仮名交じり文日本語中文한국어テスト端末出力速度"


def ascii_text(rng):
    out = []
    size = 0
    while size < SIZE:
        line = " ".join(rng.choice(WORDS) for _ in range(rng.randint(4, 24))) + "\n"
        out.append(line)
        size += len(line)
    return "".join(out).encode()


def log_text(rng):
    out = []
    size = 0
    n = 0
    levels = ["INFO", "WARN", "DEBUG", "ERROR"]
    while size < SIZE:
        n += 1
        line = "2026-10-30T12:%02d:%02d.%06dZ %-5s [worker-%d] %s id=%08x path=/api/v1/%s/%d\n" % (
            n // 60 % 60,
            n % 60,
            rng.randint(0, 999999),
            rng.choice(levels),
            rng.randint(0, 15),
            " ".join(rng.choice(WORDS) for _ in range(rng.randint(3, 10))),
            rng.getrandbits(32),
            rng.choice(WORDS),
            rng.randint(0, 99999),
        )
        out.append(line)
        size += len(line)
    return "".join(out).encode()


def color_text(rng):
    out = []
    size = 0
    while size < SIZE:
        parts = []
        for _ in range(rng.randint(4, 16)):
            fg = rng.choice(["31", "32", "33", "34", "35", "36", "1;37", "38;5;%d" % rng.randint(0, 255), "38;2;%d;%d;%d" % (rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255))])
            parts.append("\x1b[%sm%s\x1b[0m" % (fg, rng.choice(WORDS)))
        line = " ".join(parts) + "\n"
        out.append(line)
        size += len(line.encode())
    return "".join(out).encode()


def unicode_text(rng):
    out = []
    size = 0
    while size < SIZE:
        parts = []
        for _ in range(rng.randint(4, 16)):
            if rng.random() < 0.5:
                parts.append("".join(rng.choice(CJK) for _ in range(rng.randint(1, 6))))
            else:
                parts.append(rng.choice(WORDS))
        line = " ".join(parts) + "\n"
        out.append(line)
        size += len(line.encode())
    return "".join(out).encode()


def main():
    dest = sys.argv[1] if len(sys.argv) > 1 else "."
    os.makedirs(dest, exist_ok=True)
    for name, gen in [("ascii", ascii_text), ("log", log_text), ("color", color_text), ("unicode", unicode_text)]:
        path = os.path.join(dest, name + ".txt")
        if os.path.exists(path) and os.path.getsize(path) >= SIZE:
            continue
        with open(path, "wb") as f:
            f.write(gen(random.Random(42)))
        print(path)


if __name__ == "__main__":
    main()
