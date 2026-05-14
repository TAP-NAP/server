#!/usr/bin/env bash
set -euo pipefail

# Deployment constants. Most app configuration comes from .env; these values
# describe this server's Docker topology.
APP_IMAGE="tap-app-attest-server:latest"
APP_CONTAINER_NAME="tap-app-attest-server"
REDIS_CONTAINER_NAME="tap-app-attest-redis"
DOCKER_NETWORK_NAME="tap-app-attest-net"
REDIS_IMAGE="redis:7-bookworm"

APP_HOST_BIND="127.0.0.1"
APP_PORT="8080"
APP_CONTAINER_PORT="8080"
APP_HOST_HEALTH_URL="http://${APP_HOST_BIND}:${APP_PORT}/healthz"
APP_CONTAINER_HEALTH_URL="http://127.0.0.1:${APP_CONTAINER_PORT}/healthz"

REDIS_ADDRESS="redis://${REDIS_CONTAINER_NAME}:6379"
REDIS_DATA_DIR="/opt/tap-app-attest/data/redis"
REDIS_BACKUP_DIR="/opt/tap-app-attest/backups/redis"

HOST_ROOT_CA_PATH="certs/apple_app_attestation_root_ca.pem"
CONTAINER_ROOT_CA_PATH="/app/certs/apple_app_attestation_root_ca.pem"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

log_info() {
  printf '[INFO] %s\n' "$*"
}

log_ok() {
  printf '[OK] %s\n' "$*"
}

log_warn() {
  printf '[WARN] %s\n' "$*"
}

log_error() {
  printf '[ERROR] %s\n' "$*" >&2
}

die() {
  log_error "$*"
  exit 1
}

usage() {
  cat <<EOF
TAP App Attest deployment helper

Usage:
  ./scripts/server.sh help
  ./scripts/server.sh init
  ./scripts/server.sh start [--restore-from <backup.rdb>]
  ./scripts/server.sh stop [--no-backup]
  ./scripts/server.sh backup
  ./scripts/server.sh self-test
  ./scripts/server.sh clean [--data|--all]
  ./scripts/server.sh status
  ./scripts/server.sh logs [--redis]

State semantics:
  stop
    Stops app and Redis. Redis data remains in:
      ${REDIS_DATA_DIR}

  start
    Starts Redis and app using the current Redis data directory. This is not an
    empty start after stop; existing challenge and credential records are loaded.

  start --restore-from <backup.rdb>
    Requires app and Redis to be stopped. Replaces the Redis data directory with
    the given RDB backup, then starts Redis and app.

  clean
    Removes app/Redis containers and the Docker network. Keeps Redis data and
    backups, so a later start still loads the old Redis data.

  clean --data
    Also deletes ${REDIS_DATA_DIR}. Backups remain.

  clean --all
    Deletes containers, network, app image, Redis data, and Redis backups.

Examples:
  docker build -t ${APP_IMAGE} .
  ./scripts/server.sh init
  ./scripts/server.sh start
  ./scripts/server.sh self-test
  ./scripts/server.sh backup
  ./scripts/server.sh stop
  ./scripts/server.sh start --restore-from /opt/tap-app-attest/backups/redis/redis-20260514-120000.rdb
  ./scripts/server.sh clean --data

Deployment constants:
  App image:          ${APP_IMAGE}
  App container:      ${APP_CONTAINER_NAME}
  Redis container:    ${REDIS_CONTAINER_NAME}
  Docker network:     ${DOCKER_NETWORK_NAME}
  App host URL:       ${APP_HOST_HEALTH_URL}
  Redis data dir:     ${REDIS_DATA_DIR}
  Redis backup dir:   ${REDIS_BACKUP_DIR}
EOF
}

require_no_args() {
  if [ "$#" -ne 0 ]; then
    usage
    die "Unexpected arguments: $*"
  fi
}

ensure_docker() {
  log_info "Checking Docker daemon"
  docker version >/dev/null 2>&1 || die "Docker is not reachable. Start Docker and make sure your user can run docker commands."
  log_ok "Docker is reachable"
}

load_env() {
  log_info "Checking .env"
  [ -f ".env" ] || die "Missing .env in $PROJECT_ROOT. Create it from .env.example and set TEAM_ID / BUNDLE_ID."

  # Read the app config without sourcing it, so .env cannot accidentally
  # override deployment constants such as container names or host data paths.
  local team_id
  local bundle_id
  local app_attest_env
  local server_addr
  local redis_url

  team_id="$(read_env_value TEAM_ID)"
  bundle_id="$(read_env_value BUNDLE_ID)"
  app_attest_env="$(read_env_value APP_ATTEST_ENV)"
  server_addr="$(read_env_value SERVER_ADDR)"
  redis_url="$(read_env_value REDIS_URL)"

  [ -n "$team_id" ] || die "TEAM_ID is required in .env"
  [ -n "$bundle_id" ] || die "BUNDLE_ID is required in .env"
  [ -n "$app_attest_env" ] || die "APP_ATTEST_ENV is required in .env"
  [ -n "$server_addr" ] || die "SERVER_ADDR is required in .env"
  [ -n "$redis_url" ] || die "REDIS_URL is required in .env"

  if [ "$server_addr" != "0.0.0.0:${APP_CONTAINER_PORT}" ]; then
    die "SERVER_ADDR must be 0.0.0.0:${APP_CONTAINER_PORT} for Docker deployment; current value: $server_addr"
  fi

  if [ "$redis_url" != "$REDIS_ADDRESS" ]; then
    die "REDIS_URL must be $REDIS_ADDRESS for this Docker network; current value: $redis_url"
  fi

  log_ok ".env loaded and validated for Docker deployment"
}

read_env_value() {
  local key="$1"
  local line
  line="$(grep -E "^[[:space:]]*${key}=" ".env" | tail -n 1 || true)"
  line="${line#*=}"
  line="${line%\"}"
  line="${line#\"}"
  line="${line%\'}"
  line="${line#\'}"
  printf '%s' "$line"
}

ensure_root_ca() {
  log_info "Checking Apple App Attestation Root CA"
  [ -f "$HOST_ROOT_CA_PATH" ] || die "Missing root CA file: $PROJECT_ROOT/$HOST_ROOT_CA_PATH"
  log_ok "Root CA found: $HOST_ROOT_CA_PATH"
}

ensure_app_image() {
  log_info "Checking app Docker image: $APP_IMAGE"
  if ! docker image inspect "$APP_IMAGE" >/dev/null 2>&1; then
    die "Docker image not found: $APP_IMAGE. Build it first with: docker build -t $APP_IMAGE ."
  fi
  log_ok "App image exists: $APP_IMAGE"
}

mkdir_privileged() {
  local dir="$1"
  if mkdir -p "$dir" 2>/dev/null; then
    return
  fi

  command -v sudo >/dev/null 2>&1 || die "Cannot create $dir and sudo is not available"
  sudo mkdir -p "$dir"
}

copy_privileged() {
  local src="$1"
  local dst="$2"
  if cp "$src" "$dst" 2>/dev/null; then
    return
  fi

  command -v sudo >/dev/null 2>&1 || die "Cannot copy $src to $dst and sudo is not available"
  sudo cp "$src" "$dst"
}

remove_dir_privileged() {
  local dir="$1"
  [ -e "$dir" ] || return

  if rm -rf "$dir" 2>/dev/null; then
    return
  fi

  command -v sudo >/dev/null 2>&1 || die "Cannot remove $dir and sudo is not available"
  sudo rm -rf "$dir"
}

clear_dir_privileged() {
  local dir="$1"
  mkdir_privileged "$dir"

  if find "$dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null; then
    return
  fi

  command -v sudo >/dev/null 2>&1 || die "Cannot clear $dir and sudo is not available"
  sudo find "$dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
}

assert_managed_path() {
  local path="$1"
  case "$path" in
    "$REDIS_DATA_DIR"|"$REDIS_BACKUP_DIR")
      ;;
    *)
      die "Refusing to delete unmanaged path: $path"
      ;;
  esac
}

ensure_dirs() {
  log_info "Ensuring Redis data directory: $REDIS_DATA_DIR"
  mkdir_privileged "$REDIS_DATA_DIR"
  log_ok "Redis data directory is ready"

  log_info "Ensuring Redis backup directory: $REDIS_BACKUP_DIR"
  mkdir_privileged "$REDIS_BACKUP_DIR"
  log_ok "Redis backup directory is ready"

  # Redis official images commonly run as uid 999. If chown fails, keep going;
  # Docker may still be able to write depending on the host filesystem.
  if command -v sudo >/dev/null 2>&1; then
    sudo chown -R 999:999 "$REDIS_DATA_DIR" || log_warn "Could not chown $REDIS_DATA_DIR to uid 999"
  else
    chown -R 999:999 "$REDIS_DATA_DIR" || log_warn "Could not chown $REDIS_DATA_DIR to uid 999"
  fi
}

ensure_network() {
  log_info "Checking Docker network: $DOCKER_NETWORK_NAME"
  if docker network inspect "$DOCKER_NETWORK_NAME" >/dev/null 2>&1; then
    log_ok "Docker network exists: $DOCKER_NETWORK_NAME"
  else
    docker network create "$DOCKER_NETWORK_NAME" >/dev/null
    log_ok "Docker network created: $DOCKER_NETWORK_NAME"
  fi
}

container_exists() {
  docker container inspect "$1" >/dev/null 2>&1
}

container_running() {
  [ "$(docker inspect --format '{{.State.Running}}' "$1" 2>/dev/null || true)" = "true" ]
}

container_status() {
  docker inspect --format '{{.State.Status}}' "$1" 2>/dev/null || printf 'missing'
}

validate_redis_mount() {
  if ! container_exists "$REDIS_CONTAINER_NAME"; then
    return
  fi

  if docker inspect \
    --format '{{range .Mounts}}{{.Source}}:{{.Destination}}{{"\n"}}{{end}}' \
    "$REDIS_CONTAINER_NAME" | grep -Fx "$REDIS_DATA_DIR:/data" >/dev/null; then
    return
  fi

  die "Existing Redis container does not use $REDIS_DATA_DIR:/data. Inspect it before removing it with: ./scripts/server.sh clean"
}

cmd_init() {
  require_no_args "$@"
  ensure_docker
  load_env
  ensure_root_ca
  ensure_app_image
  ensure_dirs
  ensure_network
  log_ok "Initialization complete"
}

prepare_restore() {
  local backup_file="$1"

  log_info "Preparing restore from: $backup_file"
  [ -f "$backup_file" ] || die "Backup file does not exist: $backup_file"
  [ -s "$backup_file" ] || die "Backup file is empty: $backup_file"

  if container_running "$APP_CONTAINER_NAME" || container_running "$REDIS_CONTAINER_NAME"; then
    die "Restore requires app and Redis to be stopped. Run ./scripts/server.sh stop first."
  fi

  if container_exists "$APP_CONTAINER_NAME"; then
    docker rm "$APP_CONTAINER_NAME" >/dev/null
    log_ok "Removed stopped app container before restore"
  fi

  if container_exists "$REDIS_CONTAINER_NAME"; then
    validate_redis_mount
    docker rm "$REDIS_CONTAINER_NAME" >/dev/null
    log_ok "Removed stopped Redis container before restore"
  fi

  assert_managed_path "$REDIS_DATA_DIR"
  log_warn "Replacing Redis data directory: $REDIS_DATA_DIR"
  clear_dir_privileged "$REDIS_DATA_DIR"
  copy_privileged "$backup_file" "$REDIS_DATA_DIR/dump.rdb"

  if command -v sudo >/dev/null 2>&1; then
    sudo chown -R 999:999 "$REDIS_DATA_DIR" || log_warn "Could not chown restored Redis data to uid 999"
  else
    chown -R 999:999 "$REDIS_DATA_DIR" || log_warn "Could not chown restored Redis data to uid 999"
  fi

  log_ok "Restore data prepared from: $backup_file"
}

wait_for_redis() {
  log_info "Waiting for Redis ping"
  for _ in $(seq 1 30); do
    if docker exec "$REDIS_CONTAINER_NAME" redis-cli ping >/dev/null 2>&1; then
      log_ok "Redis is ready"
      return
    fi
    sleep 1
  done

  docker logs --tail 80 "$REDIS_CONTAINER_NAME" || true
  die "Redis did not become ready"
}

start_redis() {
  validate_redis_mount

  if container_exists "$REDIS_CONTAINER_NAME"; then
    log_info "Starting existing Redis container: $REDIS_CONTAINER_NAME"
    docker start "$REDIS_CONTAINER_NAME" >/dev/null
  else
    log_info "Creating Redis container: $REDIS_CONTAINER_NAME"
    docker run -d \
      --name "$REDIS_CONTAINER_NAME" \
      --restart unless-stopped \
      --network "$DOCKER_NETWORK_NAME" \
      -v "$REDIS_DATA_DIR:/data" \
      "$REDIS_IMAGE" \
      redis-server --appendonly yes >/dev/null
  fi

  wait_for_redis
}

wait_for_app() {
  log_info "Waiting for app health check: $APP_CONTAINER_HEALTH_URL"
  for _ in $(seq 1 30); do
    if docker exec "$APP_CONTAINER_NAME" curl -fsS "$APP_CONTAINER_HEALTH_URL" >/dev/null 2>&1; then
      log_ok "App is healthy"
      return
    fi
    sleep 1
  done

  docker logs --tail 100 "$APP_CONTAINER_NAME" || true
  die "App container started but health check failed"
}

start_app() {
  if container_exists "$APP_CONTAINER_NAME"; then
    log_info "Removing existing app container so config/image changes are applied"
    docker rm -f "$APP_CONTAINER_NAME" >/dev/null
  fi

  log_info "Creating app container: $APP_CONTAINER_NAME"
  docker run -d \
    --name "$APP_CONTAINER_NAME" \
    --restart unless-stopped \
    --network "$DOCKER_NETWORK_NAME" \
    --env-file .env \
    -e APPLE_APP_ATTEST_ROOT_CA_PATH="$CONTAINER_ROOT_CA_PATH" \
    -p "${APP_HOST_BIND}:${APP_PORT}:${APP_CONTAINER_PORT}" \
    "$APP_IMAGE" >/dev/null

  wait_for_app
}

cmd_start() {
  local restore_from=""

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --restore-from)
        [ "$#" -ge 2 ] || die "--restore-from requires a backup file path"
        restore_from="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage
        die "Unknown start option: $1"
        ;;
    esac
  done

  ensure_docker
  load_env
  ensure_root_ca
  ensure_app_image
  ensure_dirs
  ensure_network

  if [ -n "$restore_from" ]; then
    prepare_restore "$restore_from"
  fi

  start_redis
  start_app

  log_ok "Started app container: $APP_CONTAINER_NAME"
  log_ok "Started Redis container: $REDIS_CONTAINER_NAME"
  log_info "App health URL: $APP_HOST_HEALTH_URL"
  log_info "Redis data dir: $REDIS_DATA_DIR"
  log_info "Redis backup dir: $REDIS_BACKUP_DIR"
}

stop_app_if_running() {
  if container_running "$APP_CONTAINER_NAME"; then
    log_info "Stopping app container: $APP_CONTAINER_NAME"
    docker stop "$APP_CONTAINER_NAME" >/dev/null
    log_ok "App container stopped"
  elif container_exists "$APP_CONTAINER_NAME"; then
    log_warn "App container already stopped: $APP_CONTAINER_NAME"
  else
    log_warn "App container not found: $APP_CONTAINER_NAME"
  fi
}

stop_redis_if_running() {
  if container_running "$REDIS_CONTAINER_NAME"; then
    log_info "Stopping Redis container: $REDIS_CONTAINER_NAME"
    docker stop "$REDIS_CONTAINER_NAME" >/dev/null
    log_ok "Redis container stopped"
  elif container_exists "$REDIS_CONTAINER_NAME"; then
    log_warn "Redis container already stopped: $REDIS_CONTAINER_NAME"
  else
    log_warn "Redis container not found: $REDIS_CONTAINER_NAME"
  fi
}

cmd_backup() {
  require_no_args "$@"
  ensure_docker
  mkdir_privileged "$REDIS_BACKUP_DIR"

  if ! container_running "$REDIS_CONTAINER_NAME"; then
    die "Redis is not running; cannot create a live backup"
  fi

  log_info "Saving Redis RDB snapshot with redis-cli SAVE"
  docker exec "$REDIS_CONTAINER_NAME" redis-cli SAVE >/dev/null
  log_ok "Redis SAVE completed"

  local source_rdb="$REDIS_DATA_DIR/dump.rdb"
  [ -f "$source_rdb" ] || die "Redis did not produce $source_rdb"

  local timestamp
  timestamp="$(date +%Y%m%d-%H%M%S)"
  local target="$REDIS_BACKUP_DIR/redis-${timestamp}.rdb"

  copy_privileged "$source_rdb" "$target"
  if command -v sudo >/dev/null 2>&1; then
    sudo chown "$(id -u):$(id -g)" "$target" || true
  fi

  log_ok "Redis backup written: $target"
}

cmd_stop() {
  local no_backup=0

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --no-backup)
        no_backup=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage
        die "Unknown stop option: $1"
        ;;
    esac
  done

  ensure_docker

  # Stop the app first so the Redis backup is not racing active app writes.
  stop_app_if_running

  if [ "$no_backup" = "1" ]; then
    log_warn "Skipping Redis backup because --no-backup was provided"
  elif container_running "$REDIS_CONTAINER_NAME"; then
    cmd_backup
  else
    log_warn "Skipping Redis backup because Redis is not running"
  fi

  stop_redis_if_running
  log_ok "Stop complete. Redis data remains at: $REDIS_DATA_DIR"
}

remove_container_if_exists() {
  local name="$1"
  if container_exists "$name"; then
    log_info "Removing container: $name"
    docker rm -f "$name" >/dev/null
    log_ok "Container removed: $name"
  else
    log_warn "Container not found: $name"
  fi
}

remove_network_if_exists() {
  if docker network inspect "$DOCKER_NETWORK_NAME" >/dev/null 2>&1; then
    log_info "Removing Docker network: $DOCKER_NETWORK_NAME"
    docker network rm "$DOCKER_NETWORK_NAME" >/dev/null
    log_ok "Docker network removed"
  else
    log_warn "Docker network not found: $DOCKER_NETWORK_NAME"
  fi
}

remove_image_if_exists() {
  if docker image inspect "$APP_IMAGE" >/dev/null 2>&1; then
    log_info "Removing app image: $APP_IMAGE"
    docker rmi "$APP_IMAGE" >/dev/null
    log_ok "App image removed: $APP_IMAGE"
  else
    log_warn "App image not found: $APP_IMAGE"
  fi
}

cmd_clean() {
  local delete_data=0
  local delete_all=0

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --data)
        delete_data=1
        shift
        ;;
      --all)
        delete_data=1
        delete_all=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage
        die "Unknown clean option: $1"
        ;;
    esac
  done

  ensure_docker
  remove_container_if_exists "$APP_CONTAINER_NAME"
  remove_container_if_exists "$REDIS_CONTAINER_NAME"
  remove_network_if_exists

  if [ "$delete_data" = "1" ]; then
    assert_managed_path "$REDIS_DATA_DIR"
    log_warn "Deleting Redis data directory: $REDIS_DATA_DIR"
    remove_dir_privileged "$REDIS_DATA_DIR"
    log_ok "Redis data directory deleted"
  else
    log_ok "Redis data kept: $REDIS_DATA_DIR"
  fi

  if [ "$delete_all" = "1" ]; then
    remove_image_if_exists
    assert_managed_path "$REDIS_BACKUP_DIR"
    log_warn "Deleting Redis backup directory: $REDIS_BACKUP_DIR"
    remove_dir_privileged "$REDIS_BACKUP_DIR"
    log_ok "Redis backup directory deleted"
  else
    log_ok "Redis backups kept: $REDIS_BACKUP_DIR"
  fi

  log_ok "Clean complete"
}

cmd_status() {
  require_no_args "$@"
  ensure_docker

  local latest_backup=""
  latest_backup="$(ls -1t "$REDIS_BACKUP_DIR"/*.rdb 2>/dev/null | head -n 1 || true)"

  printf 'App container:      %s (%s)\n' "$APP_CONTAINER_NAME" "$(container_status "$APP_CONTAINER_NAME")"
  printf 'Redis container:    %s (%s)\n' "$REDIS_CONTAINER_NAME" "$(container_status "$REDIS_CONTAINER_NAME")"
  printf 'Docker network:     %s\n' "$DOCKER_NETWORK_NAME"
  printf 'App health URL:     %s\n' "$APP_HOST_HEALTH_URL"
  printf 'Redis data dir:     %s\n' "$REDIS_DATA_DIR"
  printf 'Redis backup dir:   %s\n' "$REDIS_BACKUP_DIR"
  if [ -n "$latest_backup" ]; then
    printf 'Latest backup:      %s\n' "$latest_backup"
  else
    printf 'Latest backup:      none\n'
  fi
}

cmd_logs() {
  local target="$APP_CONTAINER_NAME"

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --redis)
        target="$REDIS_CONTAINER_NAME"
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage
        die "Unknown logs option: $1"
        ;;
    esac
  done

  ensure_docker
  container_exists "$target" || die "Container not found: $target"
  log_info "Following logs for: $target"
  docker logs -f "$target"
}

cmd_self_test() {
  require_no_args "$@"
  ensure_docker
  load_env

  container_running "$REDIS_CONTAINER_NAME" || die "Redis is not running. Start the service first: ./scripts/server.sh start"
  container_running "$APP_CONTAINER_NAME" || die "App is not running. Start the service first: ./scripts/server.sh start"

  log_info "Checking Redis ping"
  if [ "$(docker exec "$REDIS_CONTAINER_NAME" redis-cli ping)" != "PONG" ]; then
    die "Redis ping failed"
  fi
  log_ok "Redis ping returned PONG"

  log_info "Checking app health inside the app container"
  docker exec "$APP_CONTAINER_NAME" curl -fsS "$APP_CONTAINER_HEALTH_URL" >/dev/null
  log_ok "App health check passed"

  log_info "Checking OpenAPI JSON"
  local openapi_response
  openapi_response="$(docker exec "$APP_CONTAINER_NAME" curl -fsS "http://127.0.0.1:${APP_CONTAINER_PORT}/openapi.json")"
  case "$openapi_response" in
    *'"openapi"'*'"/app-attest/challenges"'*)
      log_ok "OpenAPI JSON returned the expected API paths"
      ;;
    *)
      printf '%s\n' "$openapi_response"
      die "OpenAPI JSON did not include the expected fields"
      ;;
  esac

  log_info "Checking Swagger UI page"
  local swagger_response
  swagger_response="$(docker exec "$APP_CONTAINER_NAME" curl -fsS "http://127.0.0.1:${APP_CONTAINER_PORT}/swagger-ui")"
  case "$swagger_response" in
    *'SwaggerUIBundle'*'/openapi.json'*)
      log_ok "Swagger UI page returned successfully"
      ;;
    *)
      printf '%s\n' "$swagger_response"
      die "Swagger UI page did not include the expected boot script"
      ;;
  esac

  log_info "Requesting an attestation challenge"
  local challenge_response
  challenge_response="$(docker exec "$APP_CONTAINER_NAME" curl -fsS \
    -X POST "http://127.0.0.1:${APP_CONTAINER_PORT}/app-attest/challenges" \
    -H "Content-Type: application/json" \
    -d '{"purpose":"attestation","credentialName":"self_test"}')"

  case "$challenge_response" in
    *'"challengeId"'*'"challenge"'*'"expiresAt"'*)
      log_ok "Challenge endpoint returned the expected JSON shape"
      ;;
    *)
      printf '%s\n' "$challenge_response"
      die "Challenge endpoint response did not include challengeId, challenge, and expiresAt"
      ;;
  esac

  local challenge_id
  challenge_id="$(printf '%s' "$challenge_response" | sed -nE 's/.*"challengeId"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p')"
  [ -n "$challenge_id" ] || die "Could not parse challengeId from challenge response"

  local challenge_key="tap:challenge:${challenge_id}"
  log_info "Checking Redis key written by challenge endpoint: $challenge_key"
  if [ "$(docker exec "$REDIS_CONTAINER_NAME" redis-cli EXISTS "$challenge_key")" != "1" ]; then
    die "Challenge key was not found in Redis: $challenge_key"
  fi

  local ttl
  ttl="$(docker exec "$REDIS_CONTAINER_NAME" redis-cli TTL "$challenge_key")"
  if [ "$ttl" -le 0 ]; then
    die "Challenge key exists but has invalid TTL: $ttl"
  fi
  log_ok "Challenge key exists with TTL ${ttl}s"

  log_info "Removing self-test challenge key"
  docker exec "$REDIS_CONTAINER_NAME" redis-cli DEL "$challenge_key" >/dev/null || log_warn "Could not delete self-test key: $challenge_key"

  log_ok "Self-test passed"
}

main() {
  local command="${1:-help}"
  if [ "$#" -gt 0 ]; then
    shift
  fi

  case "$command" in
    help|-h|--help)
      require_no_args "$@"
      usage
      ;;
    init)
      cmd_init "$@"
      ;;
    start)
      cmd_start "$@"
      ;;
    stop)
      cmd_stop "$@"
      ;;
    backup)
      cmd_backup "$@"
      ;;
    self-test)
      cmd_self_test "$@"
      ;;
    clean)
      cmd_clean "$@"
      ;;
    status)
      cmd_status "$@"
      ;;
    logs)
      cmd_logs "$@"
      ;;
    *)
      usage
      die "Unknown command: $command"
      ;;
  esac
}

main "$@"
