#!/usr/bin/env bash
# CRFR Demo Output Script — called by VHS tape or standalone
# All data from real tests (CRFR whitepaper Section 6, April 2026)
#
# Usage: bash crfr-10questions-stream.sh
# Timing: ~11s total output, designed for 15s VHS tape

set -e

# ANSI colors
R='\033[0m'        # reset
DIM='\033[2m'      # dim
BOLD='\033[1m'
GREEN='\033[32m'
CYAN='\033[36m'
YELLOW='\033[33m'
WHITE='\033[97m'
GRAY='\033[90m'
BAR_FULL='\033[48;2;88;166;255m'  # blue bg
BAR_CRFR='\033[48;2;57;211;83m'   # green bg

S=0.07  # base delay between chars in result lines

# ── Header ──
sleep 0.1
echo ""
printf "${BOLD}${CYAN}  CRFR${R}${DIM} — Causal Resonance Field Retrieval${R}\n"
printf "${DIM}  10 real questions · 10 live websites · April 2026${R}\n"
echo ""
sleep 0.3

# ── Column headers ──
printf "${DIM}  %-42s %-14s %10s %7s  %s${R}\n" \
       "Question" "Source" "Raw" "CRFR" "Time"
printf "${DIM}  ──────────────────────────────────────── ────────────── ────────── ─────── ─────${R}\n"
sleep 0.2

# ── Results function ──
show_result() {
    local question="$1"
    local source="$2"
    local raw="$3"
    local crfr="$4"
    local time="$5"
    local answer="$6"
    local delay="${7:-0.5}"

    printf "  ${WHITE}%-42s${R} ${CYAN}%-14s${R} %10s ${GREEN}%7s${R}  ${YELLOW}%s${R}\n" \
           "$question" "$source" "$raw" "$crfr" "$time"
    if [ -n "$answer" ]; then
        printf "  ${DIM}  → %s${R}\n" "$answer"
    fi
    sleep "$delay"
}

# ── 10 Questions (real data from whitepaper Section 6) ──

show_result \
    "Sweden's inflation rate?" \
    "riksbanken.se" \
    "628,407" \
    "486" \
    "7ms" \
    '"KPIF feb 2026: 1.7% | Styrränta 1.75%"' \
    0.7

show_result \
    "Latest Python version?" \
    "python.org" \
    "335,934" \
    "270" \
    "19ms" \
    '"3.14 bugfix (2025-10-07)"' \
    0.7

show_result \
    "Population of Gothenburg?" \
    "Wikipedia SV" \
    "1,330,344" \
    "388" \
    "127ms" \
    '"674,529 (2023), Greater: 1,090,000"' \
    0.7

show_result \
    "Latest AI/tech news?" \
    "BBC RSS" \
    "6,300" \
    "331" \
    "3ms" \
    '"AI already in use in healthcare..."' \
    0.7

show_result \
    "Stockholm transit pass cost?" \
    "SL.se" \
    "(SPA)" \
    "detect" \
    "0ms" \
    '"spa_detected → fallback → 1,060 kr/mo"' \
    0.7

show_result \
    "What is ibuprofen?" \
    "Wikipedia EN" \
    "1,044,595" \
    "380" \
    "96ms" \
    '"NSAID for pain, fever, inflammation"' \
    0.7

show_result \
    "What is the EU AI Act?" \
    "Wikipedia EN" \
    "473,911" \
    "490" \
    "52ms" \
    "" \
    0.5

show_result \
    "Current S&P 500 value?" \
    "Search" \
    "24,000" \
    "280" \
    "203ms" \
    "" \
    0.5

show_result \
    "What is the universe made of?" \
    "Wikipedia EN" \
    "1,339,506" \
    "380" \
    "146ms" \
    '"5% matter, 27% dark matter, 68% dark energy"' \
    0.7

show_result \
    "Champions League 2025-26?" \
    "Wikipedia EN" \
    "1,210,820" \
    "310" \
    "97ms" \
    "" \
    0.3

# ── Summary ──
echo ""
sleep 0.2

# Totals bar
printf "  ${DIM}──────────────────────────────────────────────────────────────────────────${R}\n"
sleep 0.1

printf "  ${BOLD}${WHITE}TOTAL: 6,405,817 chars in → 3,469 chars out${R}"
printf "                    ${BOLD}${GREEN}99.9%% reduction${R}\n"
sleep 0.3

# Visual bar comparison
printf "\n"
printf "  ${BAR_FULL}                                                              ${R} ${DIM}677k tokens  \$4.00${R}\n"
printf "  ${BAR_CRFR} ${R} ${DIM}869 tokens  \$0.002${R}\n"
sleep 0.3

# Punchline
printf "\n"
printf "  ${BOLD}10/10 correct${R} ${DIM}·${R} ${BOLD}avg 90ms${R} ${DIM}·${R} ${BOLD}no GPU${R} ${DIM}·${R} ${BOLD}no embeddings${R} ${DIM}·${R} ${BOLD}1.8MB binary${R}\n"
printf "  ${DIM}pure Rust · sub-ms cached · learns from interaction${R}\n"
echo ""
