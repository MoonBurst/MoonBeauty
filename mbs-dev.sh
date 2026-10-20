#!/usr/bin/env bash
# mbs-dev.sh — dev loop. Kills matching processes, rebuilds, launches
# detached via `nix develop` so LD_LIBRARY_PATH is set and the parent
# terminal is never touched.
#
#   mbs-dev.sh                        restart bar with the new binary
#   mbs-dev.sh --screenshot           run screenshot tool (bar untouched)
#   mbs-dev.sh --screenshot --annotate  run annotate tool (bar untouched)
set -uo pipefail
cd "$(dirname "$0")"

# ── Kill matching processes ──────────────────────────────────────────
echo "== killing matching processes =="
if [ $# -eq 0 ]; then
    pkill -9 -f 'moon-beauty-shell$'                 2>/dev/null || true
    pkill -9 -f 'moon-beauty-shell --notif-surface'  2>/dev/null || true
elif [[ "$*" == *"--screenshot"* ]]; then
    pkill -9 -f 'moon-beauty-shell --annotator-session' 2>/dev/null || true
    pkill -9 -f 'moon-beauty-shell --screenshot'        2>/dev/null || true
else
    pkill -9 -f 'moon-beauty-shell' 2>/dev/null || true
fi
sleep 0.5
if pgrep -f 'moon-beauty-shell' >/dev/null; then
    echo "  ⚠️  some moon-beauty-shell still running:"
    pgrep -af 'moon-beauty-shell' | head -5
else
    echo "  ✅ all clear"
fi

# ── Build ────────────────────────────────────────────────────────────
echo ""
echo "== building =="
if ! nix develop --command bash -c 'cargo build --release 2>&1 | tail -4'; then
    echo "❌ build failed" >&2
    exit 1
fi

# ── Launch: full session isolation, no pty inheritance ───────────────
echo ""
echo "== launching: moon-beauty-shell $* =="
: > /tmp/mbs-run.log
# setsid --fork = new session leader, no controlling tty
# nohup + </dev/null = HUP-proof, stdin closed
# >/tmp/log = stdout/stderr captured for later inspection
nohup setsid --fork nix develop --command ./target/release/moon-beauty-shell "$@" \
    </dev/null >/tmp/mbs-run.log 2>&1 &
sleep 4

# ── Report ───────────────────────────────────────────────────────────
echo ""
echo "== running processes =="
if pgrep -af 'moon-beauty-shell' | grep -v mbs-dev | head -5; then
    :
else
    echo "  (none)"
fi

echo ""
echo "== log (first 30 lines) =="
head -30 /tmp/mbs-run.log 2>/dev/null || echo "  (empty)"

echo ""
if pgrep -f 'moon-beauty-shell' | grep -v mbs-dev >/dev/null; then
    echo "✅ launched"
else
    echo "❌ nothing running — see log above"
    exit 1
fi
