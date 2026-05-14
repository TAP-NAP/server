#!/usr/bin/env bash
set -euo pipefail

# Hardcoded deployment knobs. Edit these values on the server if you want a
# different image, port, container name, or Redis data path.
APP_IMAGE="tap-app-attest-server:latest"
APP_CONTAINER_NAME="tap-app-attest-server"
REDIS_CONTAINER_NAME="tap-app-attest-redis"
DOCKER_NETWORK_NAME="tap-app-attest-net"
APP_PORT="8080"
APP_HEALTH_URL="http://127.0.0.1:8080/healthz"
RECREATE_REDIS="${RECREATE_REDIS:-0}"

# The app container uses this Docker-network address to reach Redis.
REDIS_ADDRESS="redis://tap-app-attest-redis:6379"

# Redis AOF/RDB files are stored on the host here, not only inside the Redis
# container. This path is intentionally fixed for the deployment server.
REDIS_DATA_DIR="/opt/tap-app-attest/data/redis"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

if [ ! -f ".env" ]; then
  echo "Missing .env in $PROJECT_ROOT"
  echo "Create it from .env.example and set TEAM_ID / BUNDLE_ID first."
  exit 1
fi

set -a
source .env
set +a

: "${TEAM_ID:?TEAM_ID is required in .env}"
: "${BUNDLE_ID:?BUNDLE_ID is required in .env}"
: "${APP_ATTEST_ENV:?APP_ATTEST_ENV is required in .env}"

docker version >/dev/null

if ! docker image inspect "$APP_IMAGE" >/dev/null 2>&1; then
  echo "Docker image not found: $APP_IMAGE"
  echo "Build it first:"
  echo "  docker build -t $APP_IMAGE ."
  exit 1
fi

if command -v sudo >/dev/null 2>&1; then
  sudo mkdir -p "$REDIS_DATA_DIR"
else
  mkdir -p "$REDIS_DATA_DIR"
fi

# Redis official images commonly run as uid 999. If chown is not allowed, keep
# going; Docker may still be able to write depending on the host filesystem.
if command -v sudo >/dev/null 2>&1; then
  sudo chown -R 999:999 "$REDIS_DATA_DIR" || true
else
  chown -R 999:999 "$REDIS_DATA_DIR" || true
fi

if ! docker network inspect "$DOCKER_NETWORK_NAME" >/dev/null 2>&1; then
  docker network create "$DOCKER_NETWORK_NAME" >/dev/null
fi

recreate_redis=0
if docker container inspect "$REDIS_CONTAINER_NAME" >/dev/null 2>&1; then
  if ! docker inspect \
    --format '{{range .Mounts}}{{.Source}}:{{.Destination}}{{"\n"}}{{end}}' \
    "$REDIS_CONTAINER_NAME" | grep -Fx "$REDIS_DATA_DIR:/data" >/dev/null; then
    echo "Existing Redis container does not use $REDIS_DATA_DIR:/data."
    if [ "$RECREATE_REDIS" != "1" ]; then
      echo "Refusing to delete it automatically because it may contain data."
      echo "Inspect it first, then rerun with:"
      echo "  RECREATE_REDIS=1 ./scripts/start.sh"
      exit 1
    fi
    echo "RECREATE_REDIS=1 set; recreating Redis container."
    recreate_redis=1
  fi
fi

if [ "$recreate_redis" = "1" ]; then
  docker rm -f "$REDIS_CONTAINER_NAME" >/dev/null
fi

if ! docker container inspect "$REDIS_CONTAINER_NAME" >/dev/null 2>&1; then
  docker run -d \
    --name "$REDIS_CONTAINER_NAME" \
    --restart unless-stopped \
    --network "$DOCKER_NETWORK_NAME" \
    -v "$REDIS_DATA_DIR:/data" \
    redis:7-bookworm \
    redis-server --appendonly yes >/dev/null
else
  docker start "$REDIS_CONTAINER_NAME" >/dev/null
fi

for _ in $(seq 1 30); do
  if docker exec "$REDIS_CONTAINER_NAME" redis-cli ping >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

docker exec "$REDIS_CONTAINER_NAME" redis-cli ping >/dev/null

if docker container inspect "$APP_CONTAINER_NAME" >/dev/null 2>&1; then
  docker rm -f "$APP_CONTAINER_NAME" >/dev/null
fi

docker run -d \
  --name "$APP_CONTAINER_NAME" \
  --restart unless-stopped \
  --network "$DOCKER_NETWORK_NAME" \
  --env-file .env \
  -e SERVER_ADDR=0.0.0.0:8080 \
  -e REDIS_URL="$REDIS_ADDRESS" \
  -e APPLE_APP_ATTEST_ROOT_CA_PATH=/app/certs/apple_app_attestation_root_ca.pem \
  -p "$APP_PORT:8080" \
  "$APP_IMAGE" >/dev/null

for _ in $(seq 1 30); do
  if docker exec "$APP_CONTAINER_NAME" curl -fsS "$APP_HEALTH_URL" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

if ! docker exec "$APP_CONTAINER_NAME" curl -fsS "$APP_HEALTH_URL" >/dev/null 2>&1; then
  echo "App container started but health check failed."
  echo
  docker logs --tail 80 "$APP_CONTAINER_NAME" || true
  exit 1
fi

docker ps --filter "name=$REDIS_CONTAINER_NAME" --filter "name=$APP_CONTAINER_NAME"

echo
echo "Started app container:   $APP_CONTAINER_NAME"
echo "Started Redis container: $REDIS_CONTAINER_NAME"
echo "Redis address for app:   $REDIS_ADDRESS"
echo "Redis host data dir:     $REDIS_DATA_DIR"
echo
echo "Health check:"
echo "  curl http://127.0.0.1:$APP_PORT/healthz"
echo
echo "Logs:"
echo "  docker logs -f $APP_CONTAINER_NAME"
echo
echo "Stop:"
echo "  ./scripts/stop.sh"
