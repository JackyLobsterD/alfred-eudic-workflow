#!/usr/bin/env bash
set -euo pipefail

Eudic_ID=$(osascript -e 'id of app "Eudb_en_free"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudb_en"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudic"' 2>/dev/null)

if [[ -z "$Eudic_ID" ]]; then
    osascript -e 'display dialog "Please install EuDic"'
    exit
fi

# A non-first list item passes the absolute path of the per-spell
# preview HTML (preview-<sanitized-spell>.html). Just open it with
# the user's default browser/viewer; no Eudic lookup needed.
arg="${1:-}"
case "$arg" in
    /*.html|file:///*)
        open "$arg"
        exit 0
        ;;
esac

# Word + app id are passed via env vars (read with `system attribute`),
# never interpolated into the script source, so a crafted query can't
# inject AppleScript. We use a two-step activation: `open -b <bundleid>`
# reliably brings the app to the foreground (in-process `activate` is
# sometimes ignored when called via dynamic `tell application id`),
# then the raw Apple event (`show dic` = «event cmddicsh», param
# «class word») triggers the lookup.
EUDIC_QUERY_WORD="$arg" EUDIC_APP_ID="$Eudic_ID" osascript <<'EOF'
set appId to (system attribute "EUDIC_APP_ID")
set theWord to (system attribute "EUDIC_QUERY_WORD")
do shell script "open -b " & quoted form of appId
tell application id appId
    activate
    «event cmddicsh» given «class word»:theWord
end tell
EOF
