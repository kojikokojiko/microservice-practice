#!/usr/bin/env bash
# Generate a JWT for testing. Usage: ./scripts/gen-jwt.sh <role> [secret]
# role: admin | teacher | student

set -e
base64url() { base64 | tr -d '\n' | tr '+/' '-_' | tr -d '='; }

ROLE="${1:-admin}"
SECRET="${2:-your-jwt-secret-change-in-production}"
EXP=$(($(date +%s) + 86400))  # 24h
HEADER='{"alg":"HS256","typ":"JWT"}'
PAYLOAD=$(printf '{"sub":"test-user","role":"%s","exp":%d}' "$ROLE" "$EXP")
B64H=$(echo -n "$HEADER" | base64 | tr -d '\n' | tr '+/' '-_' | tr -d '=')
B64P=$(echo -n "$PAYLOAD" | base64 | tr -d '\n' | tr '+/' '-_' | tr -d '=')
UNSIGNED="${B64H}.${B64P}"
SIG=$(echo -n "$UNSIGNED" | openssl dgst -sha256 -hmac "$SECRET" -binary | base64 | tr -d '\n' | tr '+/' '-_' | tr -d '=')
echo "${UNSIGNED}.${SIG}"
