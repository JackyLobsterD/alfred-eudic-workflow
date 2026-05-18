#!/usr/bin/env bash
set -euo pipefail

Eudic_ID=$(osascript -e 'id of app "Eudb_en_free"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudb_en"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudic"' 2>/dev/null)

if [[ -z "$Eudic_ID" ]]; then
    osascript -e 'display dialog "Please install EuDic"'
    exit
fi

# Word + app id are passed via env vars (read with `system attribute`),
# never interpolated into the script source, so a crafted query can't
# inject AppleScript. The app id is a runtime value, so `tell application
# id appId` does not load Eudic's scripting terminology at compile time —
# use the raw Apple event (`show dic` = «event cmddicsh», param «class
# word») which needs no terminology.
EUDIC_QUERY_WORD="${1:-}" EUDIC_APP_ID="$Eudic_ID" osascript <<'EOF'
set appId to (system attribute "EUDIC_APP_ID")
set theWord to (system attribute "EUDIC_QUERY_WORD")
tell application id appId
    activate
    «event cmddicsh» given «class word»:theWord
end tell
EOF
