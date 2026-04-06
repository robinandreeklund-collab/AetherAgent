#!/usr/bin/env bash
# CRFR 10-iteration convergence test against local server
# Protocol: Q1=baseline, Q2-Q7=train+feedback, Q8-Q10=test
set -euo pipefail

BASE="http://localhost:3000"
URL="https://www.espn.com/"
KEYWORDS='["score","game","win","loss","team","match","nba","nfl","mlb"]'

GOALS=(
  "latest sports scores today"
  "todays game results"
  "live sports scores and updates"
  "major sports results today"
  "current game scores"
  "todays match results"
  "sports scores and highlights"
  "what are todays sports results"
  "live game updates and scores"
  "current sports standings and scores"
)

# Step 1: Fetch HTML once
echo "=== Fetching $URL ==="
HTML=$(curl -s "$BASE/api/fetch" -H 'Content-Type: application/json' \
  -d "{\"url\":\"$URL\"}" | python3 -c "
import json,sys
d=json.load(sys.stdin)
print(d.get('body',''))
")
BODY_LEN=${#HTML}
echo "Fetched $BODY_LEN chars"
echo ""

for i in $(seq 0 9); do
  GOAL="${GOALS[$i]}"
  if [ $i -eq 0 ]; then PHASE="BASELINE"
  elif [ $i -le 6 ]; then PHASE="TRAIN"
  else PHASE="TEST"
  fi

  # Parse
  RESULT=$(curl -s "$BASE/api/parse-crfr" -H 'Content-Type: application/json' \
    -d "$(python3 -c "
import json
print(json.dumps({
  'html': '''$( echo "$HTML" | head -c 500000 | python3 -c "import sys; print(sys.stdin.read().replace('\\\\','\\\\\\\\').replace('\"','\\\\\"').replace('\n','\\\\n')[:400000])" )''',
  'url': '$URL',
  'goal': '$GOAL',
  'top_n': 5,
  'run_js': False
}))" 2>/dev/null || echo '{}')

  # Extract metrics
  python3 -c "
import json,sys
try:
    d = json.loads('''$RESULT''')
except:
    print('  PARSE ERROR')
    sys.exit(0)
nodes = d.get('nodes',[])
crfr = d.get('crfr',{})
fq = crfr.get('field_queries',0)
cache = crfr.get('cache_hit',False)

# Count relevant
keywords = $KEYWORDS
relevant = 0
for n in nodes:
    label = n.get('label','').lower()
    if any(k in label for k in keywords):
        relevant += 1

causal = sum(1 for n in nodes if n.get('causal_boost',0) > 0)
max_cb = max((n.get('causal_boost',0) for n in nodes), default=0)
print(f'  $PHASE Q{$i+1}: fq={fq} cache={cache} nodes={len(nodes)} rel={relevant} causal={causal} max_cb={max_cb:.4f} \"{\"$GOAL\"[:40]}\"')
for j,n in enumerate(nodes[:3]):
    label = n.get('label','')[:70]
    print(f'    #{j+1} amp={n[\"amplitude\"]:.3f} cb={n[\"causal_boost\"]:.4f} [{n[\"resonance_type\"]}] {n[\"role\"]}: \"{label}\"')
" 2>/dev/null || echo "  Q$((i+1)): ERROR"

  # Feedback only in TRAIN phase
  if [ $i -ge 1 ] && [ $i -le 6 ]; then
    # Get relevant node IDs
    FEEDBACK_IDS=$(python3 -c "
import json
try:
    d = json.loads('''$RESULT''')
except:
    print('[]')
    exit()
keywords = $KEYWORDS
ids = [n['id'] for n in d.get('nodes',[]) if any(k in n.get('label','').lower() for k in keywords)]
print(json.dumps(ids[:5]))
" 2>/dev/null || echo '[]')

    if [ "$FEEDBACK_IDS" != "[]" ]; then
      FB_RESULT=$(curl -s "$BASE/api/crfr-feedback" -H 'Content-Type: application/json' \
        -d "{\"url\":\"$URL\",\"goal\":\"$GOAL\",\"successful_node_ids\":$FEEDBACK_IDS}")
      echo "    FEEDBACK: $FEEDBACK_IDS → $(echo $FB_RESULT | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("status","?"))' 2>/dev/null || echo 'err')"
    else
      echo "    FEEDBACK: no relevant nodes to feedback"
    fi
  fi
  echo ""
done
