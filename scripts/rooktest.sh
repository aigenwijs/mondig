#!/usr/bin/env bash
# Rooktest van een gebouwde app: start het programma met MONDIG_ROOKTEST=1 en
# controleer dat het "rooktest: gestart" print en met exitcode 0 stopt.
# Gebruik: scripts/rooktest.sh <pad naar het programma> [max seconden, standaard 60]
# Werkt in Git Bash (Windows), op macOS (geen `timeout` nodig) en op Linux
# (onder xvfb-run: xvfb-run -a scripts/rooktest.sh <programma>).
set -u

bin="${1:?Gebruik: rooktest.sh <programma> [seconden]}"
max="${2:-60}"
uit="$(mktemp)"

# Uitvoer gaat naar een bestand: de Windows-release gebruikt het windows-subsysteem,
# dus alleen een omgeleide uitvoer is te lezen.
MONDIG_ROOKTEST=1 "$bin" >"$uit" 2>&1 &
pid=$!

for _ in $(seq 1 "$max"); do
  kill -0 "$pid" 2>/dev/null || break
  sleep 1
done

if kill -0 "$pid" 2>/dev/null; then
  kill "$pid" 2>/dev/null
  echo "Rooktest mislukt: programma stopte niet binnen ${max} seconden." >&2
  cat "$uit"
  exit 1
fi

code=0
wait "$pid" || code=$?
cat "$uit"

if [ "$code" -ne 0 ]; then
  echo "Rooktest mislukt: exitcode $code in plaats van 0." >&2
  exit 1
fi
if ! grep -q "rooktest: gestart" "$uit"; then
  echo "Rooktest mislukt: 'rooktest: gestart' staat niet in de uitvoer." >&2
  exit 1
fi
echo "Rooktest geslaagd."
