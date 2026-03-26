#!/bin/bash
# Hämtar Web Platform Tests för AetherAgent
#
# Användning: ./wpt/setup.sh
#
# Laddar ner relevanta WPT-tester via sparse checkout (sparar ~99% disk)
# Totalt ~40 sviter som matchar AetherAgents implementerade APIs.
set -euo pipefail

WPT_DIR="wpt-suite"
WPT_REPO="https://github.com/web-platform-tests/wpt.git"

if [ -d "$WPT_DIR/dom" ]; then
    echo "WPT-suite redan nedladdad i $WPT_DIR/"
    echo "Ta bort katalogen och kör igen för att uppdatera."
    exit 0
fi

echo "Laddar ner WPT-tester (sparse checkout)..."
git clone --depth 1 --filter=blob:none --sparse "$WPT_REPO" "$WPT_DIR"

cd "$WPT_DIR"
git sparse-checkout set \
    resources \
    \
    dom \
    domparsing \
    domxpath \
    \
    html/dom \
    html/syntax \
    html/semantics \
    html/webappapis/timers \
    html/infrastructure \
    \
    uievents \
    pointerevents \
    touch-events \
    input-events \
    focus \
    selection \
    editing \
    \
    css/selectors \
    css/cssom \
    css/css-cascade \
    css/css-values \
    css/css-display \
    css/css-color \
    css/css-flexbox \
    \
    encoding \
    url \
    xhr \
    fetch \
    webstorage \
    FileAPI \
    streams \
    compression \
    \
    custom-elements \
    shadow-dom \
    trusted-types \
    sanitizer-api \
    inert \
    quirks \
    \
    webidl \
    ecmascript \
    webmessaging \
    requestidlecallback \
    \
    wai-aria \
    accname \
    html-aam \
    core-aam \
    \
    svg \
    mathml \
    \
    console \
    hr-time \
    user-timing \
    performance-timeline

cd ..

# Räkna tester
TOTAL=$(find "$WPT_DIR" -name "*.html" -not -path "*/resources/*" | wc -l)
echo ""
echo "WPT-suite nedladdad: $TOTAL HTML-testfiler"
echo ""
echo "Kör tester med:"
echo "  cargo run --bin aether-wpt --features js-eval,blitz,fetch -- $WPT_DIR/dom/"
echo "  cargo run --bin aether-wpt --features js-eval,blitz,fetch -- $WPT_DIR/html/syntax/"
echo ""
echo "Nya sviter (2026-03-26):"
echo "  $WPT_DIR/uievents/          — UIEvent, MouseEvent, KeyboardEvent"
echo "  $WPT_DIR/pointerevents/     — PointerEvent"
echo "  $WPT_DIR/focus/             — Focus management"
echo "  $WPT_DIR/selection/         — Selection API"
echo "  $WPT_DIR/wai-aria/          — ARIA roles & states"
echo "  $WPT_DIR/accname/           — Accessible name computation"
echo "  $WPT_DIR/webidl/            — WebIDL type coercion"
echo "  $WPT_DIR/ecmascript/        — ES features in DOM"
echo "  $WPT_DIR/css/css-cascade/   — CSS cascade & specificity"
echo "  $WPT_DIR/css/css-display/   — display property"
echo "  $WPT_DIR/trusted-types/     — TrustedTypes API"
echo ""
echo "Dashboard: docs/wpt-dashboard.md"
