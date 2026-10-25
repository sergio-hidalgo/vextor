#!/bin/sh
# Prints how much disk Vextor takes (spec 14 §6) and fails when a budget
# is missed: the release binary, the .app, the compressed .dmg, and
# Vextor's app-data folder (backups listed apart). With --target, also the
# size of target/ (build it clean first for a number worth comparing).
#
#   scripts/size-report.sh            build, bundle, measure
#   scripts/size-report.sh --target   ... and report target/
#
# macOS only; spec 15 runs the same checks on the real installers in CI.
set -eu

cd "$(dirname "$0")/.."

MB=1048576
BINARY_MAX=$((12 * MB))
APP_MAX=$((13 * MB))
DMG_MAX=$((6 * MB))
DATA_MAX=$((1 * MB))

scripts/bundle-macos.sh --dmg >/dev/null

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
data="$HOME/Library/Application Support/Vextor"

bytes_of() { # a file or folder, in bytes
    if [ -d "$1" ]; then
        echo $(($(du -sk "$1" | cut -f1) * 1024))
    elif [ -e "$1" ]; then
        stat -f %z "$1"
    else
        echo 0
    fi
}

failed=0
row() { # name, bytes, budget (0 = none)
    mb=$(echo "scale=1; $2 / $MB" | bc)
    if [ "$3" -gt 0 ]; then
        budget=$(echo "scale=1; $3 / $MB" | bc)
        if [ "$2" -le "$3" ]; then verdict="ok"; else verdict="OVER"; failed=1; fi
        printf "%-34s %8s MB   budget %6s MB   %s\n" "$1" "$mb" "$budget" "$verdict"
    else
        printf "%-34s %8s MB\n" "$1" "$mb"
    fi
}

backups=$(bytes_of "$data/backups")
all_data=$(bytes_of "$data")
row "Release binary" "$(bytes_of target/release/vextor)" $BINARY_MAX
row "Vextor.app" "$(bytes_of target/bundle/Vextor.app)" $APP_MAX
row "Vextor-$version.dmg" "$(bytes_of "target/bundle/Vextor-$version.dmg")" $DMG_MAX
row "App data (without backups)" $((all_data - backups)) $DATA_MAX
row "Backups (limit set in the app)" "$backups" 0
recovery=$(find "$data/recovery" -name '*.vxtr' 2>/dev/null | wc -l | tr -d ' ')
echo "Autosaves waiting in recovery: $recovery (0 after a clean quit)"
if [ "${1:-}" = "--target" ]; then
    row "target/ (build folder)" "$(bytes_of target)" 0
fi

exit $failed
