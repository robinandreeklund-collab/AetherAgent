#!/bin/bash
# Hämtar Web Platform Tests för AetherAgent
#
# Användning: ./wpt/setup.sh
#
# Laddar ner relevanta WPT-tester via sparse checkout (sparar ~99% disk)
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
    dom \
    domparsing \
    encoding \
    webstorage \
    custom-elements \
    shadow-dom \
    html/dom \
    html/syntax \
    html/semantics \
    html/webappapis/timers \
    console \
    hr-time \
    url \
    css/selectors \
    css/cssom \
    xhr \
    fetch \
    resources

cd ..

# Räkna tester
TOTAL=$(find "$WPT_DIR" -name "*.html" -not -path "*/resources/*" | wc -l)
echo ""
echo "WPT-suite nedladdad: $TOTAL HTML-testfiler"
echo ""
echo "Kör tester med:"
echo "  cargo run --bin aether-wpt --features js-eval -- $WPT_DIR/dom/"
echo "  cargo run --bin aether-wpt --features js-eval -- $WPT_DIR/html/syntax/"
echo ""
echo "Dashboard: docs/wpt-dashboard.md"
