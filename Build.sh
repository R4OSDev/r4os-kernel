#!/bin/sh
set -eu

kernel_root=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
settings_file="$kernel_root/Settings.R4S"

if [ ! -f "$settings_file" ]; then
    echo "ERROR: Settings file not found: $settings_file" >&2
    exit 1
fi

contract_setting=
devkit_setting=
repositories_setting=
workspace_setting=
zig_setting=

while IFS='=' read -r key value; do
    case "$key" in
        CONTRACT_ROOT) contract_setting=$value ;;
        DEVKIT_ROOT) devkit_setting=$value ;;
        REPOSITORIES_ROOT) repositories_setting=$value ;;
        WORKSPACE_ROOT) workspace_setting=$value ;;
        ZIG_ROOT) zig_setting=$value ;;
    esac
done < "$settings_file"

require_setting() {
    if [ -z "$2" ]; then
        echo "ERROR: $1 is missing in $settings_file" >&2
        exit 1
    fi
}

resolve_path() {
    case "$2" in
        /*) printf '%s\n' "$2" ;;
        *) printf '%s/%s\n' "$1" "$2" ;;
    esac
}

require_setting WORKSPACE_ROOT "$workspace_setting"
require_setting REPOSITORIES_ROOT "$repositories_setting"
require_setting CONTRACT_ROOT "$contract_setting"
require_setting DEVKIT_ROOT "$devkit_setting"
require_setting ZIG_ROOT "$zig_setting"

workspace_root=$(resolve_path "$kernel_root" "$workspace_setting")
repositories_root=$(resolve_path "$kernel_root" "$repositories_setting")
contract_root=$(resolve_path "$repositories_root" "$contract_setting")
devkit_root=$(resolve_path "$workspace_root" "$devkit_setting")
zig_root=$(resolve_path "$devkit_root" "$zig_setting")

if [ ! -f "$contract_root/build.zig.zon" ]; then
    echo "ERROR: Contract repository not found: $contract_root" >&2
    exit 1
fi

zig_exe=$zig_root/zig
if [ ! -x "$zig_exe" ]; then
    echo "ERROR: Zig executable not found: $zig_exe" >&2
    exit 1
fi

cd "$kernel_root"
exec "$zig_exe" build "--fork=$contract_root" "$@"
