#!/bin/sh
# Checks the Quick Look extensions of the installed Vextor.app (spec 14d)
# from Terminal: Finder shows the artwork of documents Vextor saved, and the
# plain document icon, without a hang or a crash, for old, damaged and
# hostile files. `qlmanage -t` can't do this on current macOS (it only
# drives the old .qlgenerator plug-ins and hangs on app extensions), so the
# thumbnails are asked for the way Finder asks, with QLThumbnailGenerator.
#
#   scripts/check-quicklook.sh              write the samples, check them all
#   scripts/check-quicklook.sh FILE...      print what Finder shows for FILEs
#
# Run scripts/bundle-macos.sh --install first. Samples and thumbnails go to
# target/quicklook-check/ (thumbnails in its thumbs/ folder). macOS only;
# uses swiftc and pluginkit, no network.
set -eu

cd "$(dirname "$0")/.."

if [ "$(uname)" != "Darwin" ]; then
    echo "check-quicklook.sh checks macOS Quick Look; run it on a Mac." >&2
    exit 1
fi

for id in thumbnail preview; do
    if ! pluginkit -m -i "io.github.sergio-hidalgo.vextor.$id" | grep -q "vextor.$id"; then
        echo "The Vextor $id extension is not registered." >&2
        echo "Run scripts/bundle-macos.sh --install first." >&2
        exit 1
    fi
done

out=target/quicklook-check
mkdir -p "$out/thumbs"
swiftc -O -o "$out/check-thumbnail" crates/vextor-app/macos/QuickLook/check-thumbnail.swift

if [ $# -gt 0 ]; then
    "$out/check-thumbnail" -o "$out/thumbs" "$@"
    exit 0
fi

echo "Writing sample documents to $out..."
VEXTOR_QL_SAMPLES_DIR="$PWD/$out" cargo test -q -p vextor-app -- --ignored write_quicklook_samples >/dev/null

echo "Asking Quick Look for their thumbnails..."
reports="$HOME/Library/Logs/DiagnosticReports"
touch "$out/started"
failed=0
# good-shapes comes again last: the extension still works after the rest.
for file in "$out"/good-*.vxtr "$out"/bad-*.vxtr "$out/good-shapes.vxtr"; do
    result=$("$out/check-thumbnail" -o "$out/thumbs" "$file")
    case "$(basename "$file"):$result" in
        good-*:thumbnail*) verdict=ok ;;
        bad-*:icon*) verdict=ok ;;
        *) verdict=FAIL; failed=1 ;;
    esac
    printf '%-5s %s\n' "$verdict" "$result"
done

crashes=$(find "$reports" -newer "$out/started" -name 'Vextor*' 2>/dev/null || true)
if [ -n "$crashes" ]; then
    echo "FAIL  the extension crashed:" >&2
    echo "$crashes" >&2
    failed=1
fi

if [ $failed -ne 0 ]; then
    echo "Some files did not show what they should (good-*: artwork, bad-*: icon)." >&2
    exit 1
fi
echo "All good: saved documents show their artwork, the rest the document icon."
echo "Thumbnails: $out/thumbs/"
