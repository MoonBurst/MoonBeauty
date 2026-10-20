#!/usr/bin/env bash
# dev-watch.sh - Auto-rebuild and relaunch Moon-Beauty-Shell on source changes.
# Run this inside `nix develop` so the child process inherits LD_LIBRARY_PATH.

set -euo pipefail
cd "$(dirname "$0")"

if ! command -v cargo >/dev/null; then
    echo "❌ Error: cargo not found. Please run this script inside 'nix develop'."
    exit 1
fi

echo "👀 Watching src/ and Cargo.toml for changes..."
echo "Press Ctrl-C to stop."

# Kill any existing bar instance on startup
pkill -f 'target/release/moon-beauty-shell' 2>/dev/null || true

while true; do
    # Wait for changes using inotifywait if available, else fallback to sleep/poll
    if command -v inotifywait >/dev/null 2>&1; then
        inotifywait -q -r -e modify,create,delete,move src/ Cargo.toml
    else
        # Fallback polling every 2 seconds
        last_mtime=$(find src Cargo.toml -type f -printf '%T@\n' 2>/dev/null | sort -n | tail -1)
        while true; do
            sleep 2
            new_mtime=$(find src Cargo.toml -type f -printf '%T@\n' 2>/dev/null | sort -n | tail -1)
            if [ "$last_mtime" != "$new_mtime" ]; then
                break
            fi
        done
    fi

    echo "🔄 Change detected. Rebuilding..."
    
    # Kill existing bar
    pkill -f 'target/release/moon-beauty-shell' 2>/dev/null || true
    sleep 0.5
    
    # Build
    if cargo build --release; then
        echo "✅ Build successful. Relaunching bar..."
        # Launch the new binary in the background
        ./target/release/moon-beauty-shell &
    else
        echo "❌ Build failed. Waiting for next change..."
    fi
    echo "---------------------------------------------------"
done
