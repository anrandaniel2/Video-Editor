"""Move a build's failing steps into the job summary and its tail into annotations.

The Actions run's own log cannot be read back in every environment (large log downloads may be
blocked), so a failure can report itself through things the API does serve: the job summary and
`::error::` annotations.
"""

import sys

path = sys.argv[1]
lines = int(sys.argv[2]) if len(sys.argv) > 2 else 60
log = open(path, errors="replace").read().splitlines()

# The failing steps: Cargo prints `error[E…]: …` / `error: …` / `error: could not compile …`.
fails = [i for i, line in enumerate(log) if line.startswith("error")]
if fails:
    first = max(0, fails[0] - 12)
    print("=" * 70)
    print("\n".join(log[first : fails[-1] + 3]))

# The tail, in annotation-sized pieces (an annotation's message is capped).
chunks = [log[i : i + 40] for i in range(max(0, len(log) - 160), len(log), 40)]
for n, chunk in enumerate(chunks, 1):
    body = "\n".join(chunk).replace("%", "%25").replace("\r", "").replace("\n", "%0A")
    print(f"::error title=build log {n}/{len(chunks)}::{body}")
