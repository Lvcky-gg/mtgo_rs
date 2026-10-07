#!/bin/sh
# Arguments are passed separately, never interpolated into shell source.
set -eu
target=$1
new=$2
stage=$3
parent=$4
working_directory=$5
backup="$stage/previous.app"
report="$stage/error.txt"
i=0
while kill -0 "$parent" 2>/dev/null; do
    i=$((i + 1))
    if [ "$i" -gt 90 ]; then exit 1; fi
    sleep 1
done
cd "$working_directory"
if ! mv "$target" "$backup"; then
    printf '%s\n' 'Could not move the installed app. Install the update in a writable folder.' > "$report"
    exec "$target/Contents/MacOS/mtg-gui" --skip-update-once --update-error "$report"
fi
if ! mv "$new" "$target"; then
    mv "$backup" "$target"
    printf '%s\n' 'Could not install the update; restored the previous app.' > "$report"
    exec "$target/Contents/MacOS/mtg-gui" --skip-update-once --update-error "$report"
fi
# Launch directly so inherited database overrides and the working directory survive.
"$target/Contents/MacOS/mtg-gui" --skip-update-once >/dev/null 2>&1 &
# Only remove the updater's own sibling staging directory after launching.
if [ "$(dirname "$stage")" = "$(dirname "$target")" ]; then
    case "$(basename "$stage")" in
        .mtgo-update-*) rm -rf -- "$stage" || true ;;
    esac
fi
