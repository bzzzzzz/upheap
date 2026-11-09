#!/usr/bin/env bash
# Seeds a local development server with a test account and a few items.
# Test-only credentials, for local development: dev@example.test / dev-password-123
set -euo pipefail
BASE="${BASE:-http://localhost:8137}"
JAR="$(mktemp)"
trap 'rm -f "$JAR"' EXIT
req() { curl -sf -H 'X-CSRF-Protection: 1' -H 'Content-Type: application/json' -c "$JAR" -b "$JAR" "$@"; }

req "$BASE/api/v1/auth/register" -d '{"email":"dev@example.test","display_name":"Dev","username":"dev","password":"dev-password-123"}' >/dev/null \
  || req "$BASE/api/v1/auth/login" -d '{"login":"dev","password":"dev-password-123"}' >/dev/null
HEAP=$(req "$BASE/api/v1/me" | python3 -c 'import json,sys; t=json.load(sys.stdin)["teams"][0]; print(t["slug"] + "/heaps/" + t["heaps"][0]["slug"])')
in_days() { python3 -c "import datetime; print(datetime.date.today() + datetime.timedelta(days=$1))"; }
add() { req "$BASE/api/v1/teams/$HEAP/items" -d "$1" >/dev/null; }
add '{"title":"Fix flaky login test","need":"expected","effort":"s","impact":"l"}'
add "{\"title\":\"Renew TLS certificate\",\"need\":\"critical\",\"effort\":\"s\",\"impact\":\"xl\",\"due\":{\"date\":\"$(in_days 2)\",\"firm\":true}}"
add '{"title":"Write onboarding docs","need":"nice_to_have","effort":"l","impact":"m"}'
add "{\"title\":\"Plan Q1 roadmap\",\"need\":\"expected\",\"effort\":\"m\",\"impact\":\"l\",\"due\":{\"date\":\"$(in_days 20)\",\"firm\":false}}"
add '{"title":"Tidy up old feature flags","need":"idea"}'
echo "Seeded heap $HEAP. Sign in at $BASE as dev@example.test."
