#!/usr/bin/env bash
set -euo pipefail

exec nix develop --command cargo "$@"
