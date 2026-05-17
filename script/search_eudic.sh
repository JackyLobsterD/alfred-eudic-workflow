#!/usr/bin/env bash
set -euo pipefail

Eudic_ID=$(osascript -e 'id of app "Eudb_en_free"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudb_en"' 2>/dev/null) || \
    Eudic_ID=$(osascript -e 'id of app "Eudic"' 2>/dev/null)

if [[ -z "$Eudic_ID" ]]; then
    osascript -e 'display dialog "Please install EuDic"'
    exit
fi

# Pass the word via env var so AppleScript reads it as a literal, not as inline source.
EUDIC_QUERY_WORD="${1:-}" EUDIC_APP_ID="$Eudic_ID" osascript <<'EOF'
tell application "System Events"
    set appId to (system attribute "EUDIC_APP_ID")
    set theWord to (system attribute "EUDIC_QUERY_WORD")
    do shell script "open -b " & quoted form of appId
    tell application id appId
        activate
        show dic with word theWord
    end tell
end tell
EOF
