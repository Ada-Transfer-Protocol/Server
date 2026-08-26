#!/usr/bin/env bash
# AdaTP reproducible load-benchmark runner.
#
# Wraps loadtest.mjs (this directory): drives the server at one or more
# concurrency levels, samples server CPU/RAM during each run, parses the
# p50/p95/p99 latencies + throughput the load tester reports, and writes a CSV
# plus a Markdown table of REAL measured numbers.
#
# It MEASURES; it never fabricates. Run it against your own build and hardware,
# then paste the Markdown into docs/production/benchmarks.md.
#
# Usage:
#   ./run-benchmark.sh --clients 50,100,200 --duration 30 \
#       --url ws://127.0.0.1:3000/ws --server-pid 12345
#   ./run-benchmark.sh --clients 100 --docker adatp-server-adatp-server-1
#
# CPU/RAM are recorded only when the server process is observable — pass
# --server-pid <pid> (host process) or --docker <container>. Without either,
# those columns are reported as NA (honestly blank, never guessed).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- defaults --------------------------------------------------------------
URL="ws://127.0.0.1:3000/ws"
CLIENTS="50,100,200"
ROOMS=10
RATE=5
DURATION=30
USERNAME="user1"
PASSWORD="password123"
SERVER_PID=""
DOCKER_CONTAINER=""
OUT_DIR="${SCRIPT_DIR}/results"

usage() { sed -n '2,23p' "$0"; exit "${1:-0}"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --url) URL="$2"; shift 2;;
    --clients) CLIENTS="$2"; shift 2;;
    --rooms) ROOMS="$2"; shift 2;;
    --rate) RATE="$2"; shift 2;;
    --duration) DURATION="$2"; shift 2;;
    --username) USERNAME="$2"; shift 2;;
    --password) PASSWORD="$2"; shift 2;;
    --server-pid) SERVER_PID="$2"; shift 2;;
    --docker) DOCKER_CONTAINER="$2"; shift 2;;
    --out) OUT_DIR="$2"; shift 2;;
    -h|--help) usage 0;;
    *) echo "unknown arg: $1" >&2; usage 1;;
  esac
done

command -v node >/dev/null 2>&1 || { echo "error: node is required" >&2; exit 1; }

# Install the load tester's deps once (uses the committed package-lock).
if [ ! -d "${SCRIPT_DIR}/node_modules" ]; then
  echo "Installing loadtest deps (npm)..."
  ( cd "$SCRIPT_DIR" && { npm ci >/dev/null 2>&1 || npm install >/dev/null 2>&1; } )
fi

mkdir -p "$OUT_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
CSV="${OUT_DIR}/bench-${STAMP}.csv"
MD="${OUT_DIR}/bench-${STAMP}.md"
echo "clients,rooms,rate_msg_s,duration_s,connected,sent_per_s,recv_per_s,p50_ms,p95_ms,p99_ms,cpu_avg_pct,cpu_peak_pct,rss_avg_mb,rss_peak_mb" > "$CSV"

# ---- resource samplers -----------------------------------------------------
# Each appends "<cpu%> <rss_kb>" lines to $1 while flag file $2 exists.
sample_ps() {
  local out="$1" flag="$2"
  while [ -f "$flag" ]; do
    ps -o %cpu=,rss= -p "$SERVER_PID" 2>/dev/null | awk 'NF>=2 {print $1, $2}' >> "$out" || true
    sleep 1
  done
}
sample_docker() {
  local out="$1" flag="$2"
  while [ -f "$flag" ]; do
    # CPUPerc="12.34%"  MemUsage="45.6MiB / 1.9GiB"  (--no-stream blocks ~1s)
    docker stats --no-stream --format '{{.CPUPerc}} {{.MemUsage}}' "$DOCKER_CONTAINER" 2>/dev/null \
      | awk '{ cpu=$1; sub(/%/,"",cpu);
               val=$2; unit=$2;
               sub(/[0-9.]+/,"",unit); sub(/[A-Za-z]+$/,"",val);
               kb=val*1024; if(unit ~ /GiB/) kb=val*1024*1024; else if(unit ~ /KiB/) kb=val;
               print cpu, kb }' >> "$out" || true
  done
}

run_one() {
  local n="$1"
  local raw sflag sout sampler_pid=""
  raw="$(mktemp)"; sflag="$(mktemp)"; sout="$(mktemp)"
  echo ""
  echo "=== ${n} clients — ${DURATION}s @ ${RATE} msg/s over ${ROOMS} rooms ==="

  if [ -n "$SERVER_PID" ]; then
    sample_ps "$sout" "$sflag" & sampler_pid=$!
  elif [ -n "$DOCKER_CONTAINER" ]; then
    sample_docker "$sout" "$sflag" & sampler_pid=$!
  fi

  ( cd "$SCRIPT_DIR" && node loadtest.mjs \
      --url "$URL" --clients "$n" --rooms "$ROOMS" --rate "$RATE" \
      --duration "$DURATION" --username "$USERNAME" --password "$PASSWORD" ) \
      | tee "$raw" || true

  if [ -n "$sampler_pid" ]; then rm -f "$sflag"; wait "$sampler_pid" 2>/dev/null || true; fi

  # Parse loadtest.mjs output (failed command substitutions just yield empty).
  local pline p50 p95 p99 conn sent_s recv_s
  pline="$(grep -E 'latency p50/p95/p99' "$raw" | tail -1)"
  p50="$(printf '%s' "$pline" | sed -E 's/.* ([0-9]+)ms \/ ([0-9]+)ms \/ ([0-9]+)ms.*/\1/')"
  p95="$(printf '%s' "$pline" | sed -E 's/.* ([0-9]+)ms \/ ([0-9]+)ms \/ ([0-9]+)ms.*/\2/')"
  p99="$(printf '%s' "$pline" | sed -E 's/.* ([0-9]+)ms \/ ([0-9]+)ms \/ ([0-9]+)ms.*/\3/')"
  conn="$(grep -E '^connected' "$raw" | sed -E 's/connected +([0-9]+\/[0-9]+).*/\1/' | tail -1)"
  sent_s="$(grep -E '^messages sent' "$raw" | sed -E 's/.*\(([0-9]+)\/s\).*/\1/' | tail -1)"
  recv_s="$(grep -E '^messages received' "$raw" | sed -E 's/.*\(([0-9]+)\/s\).*/\1/' | tail -1)"

  # Aggregate resource samples: cpu% avg/peak, rss KB -> MB avg/peak.
  local agg cpu_avg cpu_peak rss_avg rss_peak
  if [ -s "$sout" ]; then
    agg="$(awk '{ c=$1+0; r=$2+0; cs+=c; if(c>cpk)cpk=c; rs+=r; if(r>rpk)rpk=r; n++ }
                END { if(n>0) printf "%.1f %.1f %.1f %.1f", cs/n, cpk, (rs/n)/1024, rpk/1024;
                      else printf "NA NA NA NA" }' "$sout")"
  else
    agg="NA NA NA NA"
  fi
  read -r cpu_avg cpu_peak rss_avg rss_peak <<EOF
$agg
EOF

  echo "${n},${ROOMS},${RATE},${DURATION},${conn:-NA},${sent_s:-NA},${recv_s:-NA},${p50:-NA},${p95:-NA},${p99:-NA},${cpu_avg},${cpu_peak},${rss_avg},${rss_peak}" >> "$CSV"
  rm -f "$raw" "$sflag" "$sout"
}

# Run each concurrency level.
IFS=',' read -r -a LEVELS <<< "$CLIENTS"
for n in "${LEVELS[@]}"; do run_one "$n"; done

# ---- render Markdown from the CSV -----------------------------------------
{
  echo "# AdaTP benchmark — ${STAMP}"
  echo ""
  echo "- URL: \`${URL}\`  rooms=${ROOMS}  rate=${RATE} msg/s  duration=${DURATION}s"
  if [ -n "$SERVER_PID" ]; then
    echo "- Resource source: host PID ${SERVER_PID}"
  elif [ -n "$DOCKER_CONTAINER" ]; then
    echo "- Resource source: docker container ${DOCKER_CONTAINER}"
  else
    echo "- Resource source: none (CPU/RAM = NA; pass --server-pid or --docker to record them)"
  fi
  echo "- Fill in the environment (CPU model, cores, RAM, build profile) when publishing."
  echo ""
  echo "| clients | connected | sent/s | recv/s | p50 ms | p95 ms | p99 ms | CPU avg % | CPU peak % | RSS avg MB | RSS peak MB |"
  echo "| --: | --: | --: | --: | --: | --: | --: | --: | --: | --: | --: |"
  tail -n +2 "$CSV" | awk -F, '{ printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", $1,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14 }'
} > "$MD"

echo ""
echo "Wrote:"
echo "  CSV: $CSV"
echo "  MD:  $MD"
echo ""
cat "$MD"
