"""Move a build's failing steps into the job summary and its tail into annotations.

The Actions run's own log cannot be read back in every environment (large log downloads may be
blocked), so a failure can report itself through things the API does serve: the job summary and
`::error::` annotations.
"""

import sys

path = sys.argv[1]
lines = int(sys.argv[2]) if len(sys.argv) > 2 else 60
log = open(path, errors="replace").read().splitlines()

# The failing steps: Cargo prints `error[E…]: …` / `error: …` / `error: could not compile …`;
# Kotlin and Java print `e: file://…` and `fatal error:` / `note:`.
fails = [i for i, line in enumerate(log) if line.startswith(("error", "e: ", "fatal error:"))]
if fails:
    first = max(0, fails[0] - 12)
    print("=" * 70)
    print("\n".join(log[first : fails[-1] + 3]))
    # Every failing line as its own annotation: a log whose head the tail drops (a long Kotlin
    # error list) still reports all of them.
    #
    # A step may leave at most ten annotations, so the per-line ones below are a sample: the
    # consolidated annotation carries the whole list, deduplicated and cut to an annotation's
    # size, so a run with sixty compiler errors reports sixty and not ten.
    seen, every = set(), []
    for i in fails:
        line = " ".join(log[i].split())
        if line not in seen:
            seen.add(line)
            every.append(line)
    budget = 60000
    picked = []
    for line in every:
        if len("\n".join(picked)) + len(line) + 1 > budget:
            picked.append(f"… and {len(every) - len(picked)} more")
            break
        picked.append(line)
    body = "\n".join(picked).replace("%", "%25").replace("\r", "").replace("\n", "%0A")
    print(f"::error title=all {len(every)} failures::{body}")

    for i in fails[:9]:
        body = log[i][:1200].replace("%", "%25").replace("\r", "").replace("\n", "%0A")
        print(f"::error title=failure {fails.index(i) + 1}/{len(fails)}::{body}")

# The tail, in annotation-sized pieces (an annotation's message is capped).
chunks = [log[i : i + 40] for i in range(max(0, len(log) - 160), len(log), 40)]
for n, chunk in enumerate(chunks, 1):
    body = "\n".join(chunk).replace("%", "%25").replace("\r", "").replace("\n", "%0A")
    print(f"::error title=build log {n}/{len(chunks)}::{body}")
