#!/usr/bin/env bash
# Boot Redis + two AdaTP server nodes on one backplane, then prove a room message
# crosses nodes. Also runs a control (no backplane → message must NOT cross).
# Skips cleanly if redis-server or the built Node SDK are unavailable.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
PINNED="d04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737"
REDIS_PORT=6399
PA=3194  # node A
PB=3193  # node B

command -v redis-server >/dev/null 2>&1 || { echo "SKIP: redis-server not found."; exit 0; }
BIN="${ADATP_SERVER_BIN:-$ROOT/target/debug/adatp-server}"
[ -x "$BIN" ] || { echo "SKIP: adatp-server not built at $BIN"; exit 0; }
[ -f "$ROOT/../sdks/node/dist/index.js" ] || { echo "SKIP: Node SDK not built."; exit 0; }

TMP="$(mktemp -d)"
printf '\x11%.0s' $(seq 1 32) > "$TMP/identity.key"
PIDS=()
cleanup() { for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null; done; redis-cli -p $REDIS_PORT shutdown nosave 2>/dev/null; rm -rf "$TMP"; }
trap cleanup EXIT

redis-server --port $REDIS_PORT --save "" --appendonly no --daemonize no >"$TMP/redis.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 40); do redis-cli -p $REDIS_PORT ping 2>/dev/null | grep -q PONG && break; sleep 0.1; done
redis-cli -p $REDIS_PORT ping 2>/dev/null | grep -q PONG || { echo "redis did not start"; exit 1; }

start_node() { # <port> <backplane_url_or_empty> <logfile>
  local base=(PORT="$1" HOST=127.0.0.1 AUTH_DRIVER=none MSG_RATE_LIMIT=0
             DATABASE_URL="sqlite:$TMP/n$1.db" ADATP_IDENTITY_PATH="$TMP/identity.key"
             RUST_LOG=info)
  if [ -n "$2" ]; then
    env "${base[@]}" ADATP_BACKPLANE_URL="$2" "$BIN" >"$3" 2>&1 &
  else
    env "${base[@]}" "$BIN" >"$3" 2>&1 &
  fi
  PIDS+=($!)
}
wait_listen() { for _ in $(seq 1 60); do grep -q "listening on" "$1" 2>/dev/null && return 0; sleep 0.1; done; echo "node did not start:"; cat "$1"; exit 1; }

echo "=== WITH backplane (message must cross nodes) ==="
start_node $PA "redis://127.0.0.1:$REDIS_PORT" "$TMP/a.log"; wait_listen "$TMP/a.log"
start_node $PB "redis://127.0.0.1:$REDIS_PORT" "$TMP/b.log"; wait_listen "$TMP/b.log"
grep -q "backplane active" "$TMP/a.log" && grep -q "backplane active" "$TMP/b.log" || { echo "backplane not active"; cat "$TMP/a.log"; exit 1; }
node "$HERE/cross_node.cjs" $PA $PB "$PINNED"; rc_bp=$?

echo "=== control: WITHOUT backplane (message must NOT cross) ==="
# Fresh ports so there is no kill/rebind race with the backplane nodes above.
PC=3192; PD=3191
start_node $PC "" "$TMP/c.log"; wait_listen "$TMP/c.log"
start_node $PD "" "$TMP/d.log"; wait_listen "$TMP/d.log"
if node "$HERE/cross_node.cjs" $PC $PD "$PINNED" >/dev/null 2>&1; then
  echo "  FAIL  control: message crossed nodes WITHOUT a backplane (should be isolated)"; rc_ctl=1
else
  echo "  ok  control: without a backplane, the two nodes are isolated (message did not cross)"; rc_ctl=0
fi

if [ $rc_bp -eq 0 ] && [ $rc_ctl -eq 0 ]; then
  echo "MULTI-NODE BACKPLANE VERIFIED (crosses with Redis, isolated without)."; exit 0
else
  echo "BACKPLANE TEST FAILED (bp=$rc_bp, control=$rc_ctl)."; exit 1
fi
