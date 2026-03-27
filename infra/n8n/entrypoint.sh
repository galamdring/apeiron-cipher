#!/bin/sh
set -e

DATA_DIR=/home/node/.n8n
OWNER_ID_FILE="$DATA_DIR/.owner_id"
CHECKSUM_FILE="$DATA_DIR/.workflows_checksum"
CURRENT_CHECKSUM=$(cat /workflows/*.json | md5sum | cut -d' ' -f1)

if [ -f "$OWNER_ID_FILE" ]; then
  OWNER_ID=$(cat "$OWNER_ID_FILE")
fi

NEEDS_SETUP=false
NEEDS_IMPORT=false

if [ -z "$OWNER_ID" ]; then
  NEEDS_SETUP=true
  NEEDS_IMPORT=true
elif [ ! -f "$CHECKSUM_FILE" ] || [ "$(cat "$CHECKSUM_FILE")" != "$CURRENT_CHECKSUM" ]; then
  NEEDS_IMPORT=true
fi

if [ "$NEEDS_IMPORT" = "false" ]; then
  n8n start
  exit $?
fi

if [ "$NEEDS_SETUP" = "true" ]; then
  n8n start &
  N8N_PID=$!

  until wget -qO /dev/null http://localhost:5678/healthz 2>/dev/null; do
    sleep 1
  done

  # Wait for migrations to finish — /healthz returns 200 before migrations
  # complete, so we poll /rest/settings which only works post-migration
  echo "Waiting for database migrations to complete..."
  MIGRATION_WAIT=0
  while true; do
    STATUS=$(wget -qO- --timeout=5 http://localhost:5678/rest/settings 2>/dev/null | grep -c '"communityNodesEnabled"' || echo "0")
    if [ "$STATUS" -ge 1 ]; then
      echo "Migrations complete after ${MIGRATION_WAIT}s"
      break
    fi
    MIGRATION_WAIT=$((MIGRATION_WAIT + 3))
    if [ "$MIGRATION_WAIT" -gt 600 ]; then
      echo "ERROR: Migrations did not complete within 10 minutes"
      exit 1
    fi
    sleep 3
  done

  echo "Creating owner account..."
  SETUP_RESPONSE=$(node -e "
    const http = require('http');
    const data = JSON.stringify({email:process.env.N8N_OWNER_EMAIL, firstName:'Admin', lastName:'User', password:process.env.N8N_OWNER_PASSWORD});
    const req = http.request({hostname:'localhost',port:5678,path:'/rest/owner/setup',method:'POST',headers:{'Content-Type':'application/json','Content-Length':data.length}}, res => {
      let body = '';
      res.on('data', c => body += c);
      res.on('end', () => console.log(body));
    });
    req.write(data);
    req.end();
  " 2>&1 || echo "")

  OWNER_ID=$(echo "$SETUP_RESPONSE" | grep -o '"id" *: *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')

  if [ -z "$OWNER_ID" ]; then
    echo "Owner exists, logging in..."
    OWNER_ID=$(node -e "
      const http = require('http');
      const data = JSON.stringify({emailOrLdapLoginId:process.env.N8N_OWNER_EMAIL, password:process.env.N8N_OWNER_PASSWORD});
      const req = http.request({hostname:'localhost',port:5678,path:'/rest/login',method:'POST',headers:{'Content-Type':'application/json','Content-Length':data.length}}, res => {
        let body = '';
        res.on('data', c => body += c);
        res.on('end', () => { const d = JSON.parse(body); console.log(d.data?.id || ''); });
      });
      req.write(data);
      req.end();
    " 2>&1)
  fi

  kill $N8N_PID
  wait $N8N_PID 2>/dev/null || true

  if [ -z "$OWNER_ID" ]; then
    echo "ERROR: Failed to get owner ID. Check N8N_OWNER_EMAIL and N8N_OWNER_PASSWORD."
    exit 1
  fi

  echo "$OWNER_ID" > "$OWNER_ID_FILE"
fi

if [ "$NEEDS_SETUP" = "true" ]; then
  echo "Importing workflows as owner ${OWNER_ID}..."
  n8n import:workflow --input=/workflows --separate --userId="$OWNER_ID"
else
  echo "Updating workflows..."
  n8n import:workflow --input=/workflows --separate
fi

echo "Publishing workflows..."
for WF_FILE in /workflows/*.json; do
  # Extract the workflow-level id (last "id" field in JSON, not node-level ids)
  WF_ID=$(node -e "const wf=JSON.parse(require('fs').readFileSync('$WF_FILE','utf8')); if(wf.id) console.log(wf.id);" 2>/dev/null)
  if [ -n "$WF_ID" ]; then
    echo "Publishing workflow with ID: $WF_ID"
    n8n publish:workflow --id="$WF_ID" 2>/dev/null || true
  else
    echo "Skipping $(basename $WF_FILE) — no workflow-level id"
  fi
done

echo "$CURRENT_CHECKSUM" > "$CHECKSUM_FILE"

n8n start
exit $?
