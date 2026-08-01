#!/bin/sh
set -e
if [ "${LOREMETRY_DISABLE_WORKER:-}" != "1" ]; then
  /app/loremetry-worker &
fi
exec /app/loremetry-web
