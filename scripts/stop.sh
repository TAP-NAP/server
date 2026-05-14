#!/usr/bin/env bash
set -euo pipefail

# Keep these names in sync with scripts/start.sh.
APP_CONTAINER_NAME="tap-app-attest-server"
REDIS_CONTAINER_NAME="tap-app-attest-redis"

# Set REMOVE_REDIS=1 if you want this script to remove the Redis container
# after stopping it. Redis data remains on the host bind mount either way.
REMOVE_REDIS="${REMOVE_REDIS:-0}"

docker version >/dev/null

if docker container inspect "$APP_CONTAINER_NAME" >/dev/null 2>&1; then
  docker rm -f "$APP_CONTAINER_NAME" >/dev/null
  echo "Stopped and removed app container: $APP_CONTAINER_NAME"
else
  echo "App container not found: $APP_CONTAINER_NAME"
fi

if docker container inspect "$REDIS_CONTAINER_NAME" >/dev/null 2>&1; then
  if [ "$REMOVE_REDIS" = "1" ]; then
    docker rm -f "$REDIS_CONTAINER_NAME" >/dev/null
    echo "Stopped and removed Redis container: $REDIS_CONTAINER_NAME"
  else
    docker stop "$REDIS_CONTAINER_NAME" >/dev/null
    echo "Stopped Redis container: $REDIS_CONTAINER_NAME"
  fi
else
  echo "Redis container not found: $REDIS_CONTAINER_NAME"
fi

echo
echo "Redis data was not deleted. It remains on the host bind mount configured in scripts/start.sh."
