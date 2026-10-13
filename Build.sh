#!/bin/sh
set -eu
kernel_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
exec pwsh -NoLogo -NoProfile -File "$kernel_root/Build.ps1" "$@"
