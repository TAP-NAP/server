# Production Deployment From Scratch

This guide assumes a fresh Ubuntu server, a domain name, and an already-built
Docker image for this app.

## 1. Point DNS To The Server

Create an `A` record:

```text
api.example.com -> YOUR_SERVER_IPV4
```

If you use IPv6, also create an `AAAA` record. Wait until DNS resolves:

```sh
dig +short api.example.com
```

The iOS app's `APP_ATTEST_BACKEND_URL` must use HTTPS:

```text
https://api.example.com
```

## 2. Install Docker Engine And Compose

On Ubuntu, install Docker from Docker's official apt repository:

```sh
sudo apt-get update
sudo apt-get install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
```

Let your user run Docker without `sudo`:

```sh
sudo usermod -aG docker "$USER"
newgrp docker
```

Verify:

```sh
docker version
docker compose version
```

## 3. Prepare App Directory

Choose a stable directory:

```sh
sudo mkdir -p /opt/tap-app-attest
sudo chown -R "$USER":"$USER" /opt/tap-app-attest
cd /opt/tap-app-attest
```

Copy this repository to the server, or clone it there.

## 4. Configure Environment

Create `.env`:

```sh
cat > .env <<'EOF'
APP_IMAGE=tap-app-attest-server:latest
DOMAIN=api.example.com
ACME_EMAIL=admin@example.com

TEAM_ID=YOUR_APPLE_TEAM_ID
BUNDLE_ID=com.example.tapcam
APP_ATTEST_ENV=production
CHALLENGE_TTL_SECONDS=3600

REDIS_DATA_DIR=/opt/tap-app-attest/data/redis
CADDY_DATA_DIR=/opt/tap-app-attest/data/caddy
CADDY_CONFIG_DIR=/opt/tap-app-attest/data/caddy-config
RUST_LOG=tap_app_attest_server=info,tower_http=info
EOF
```

`TEAM_ID`, `BUNDLE_ID`, `DOMAIN`, and `ACME_EMAIL` must be real values.

## 5. Persist Redis Outside The Container

The production compose file bind-mounts Redis data:

```text
${REDIS_DATA_DIR}:/data
```

Create the directories:

```sh
mkdir -p /opt/tap-app-attest/data/redis
mkdir -p /opt/tap-app-attest/data/caddy
mkdir -p /opt/tap-app-attest/data/caddy-config
```

If Redis cannot write to the bind mount, fix ownership for the Redis container
user:

```sh
sudo chown -R 999:999 /opt/tap-app-attest/data/redis
```

Do not run `docker compose down -v` unless you intend to delete Docker-managed
volumes. With the production bind mount, Redis data lives under
`/opt/tap-app-attest/data/redis`.

## 6. Open Firewall Ports

Caddy needs ports 80 and 443 to issue and renew HTTPS certificates:

```sh
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
```

Do not expose Redis publicly.

## 7. Start The Service

If the image already exists on the server:

```sh
docker compose -f deploy/compose.prod.yml up -d
```

If the image is in a registry:

```sh
docker compose -f deploy/compose.prod.yml pull
docker compose -f deploy/compose.prod.yml up -d
```

If you want to build on the server from this repository:

```sh
docker build -t tap-app-attest-server:latest .
docker compose -f deploy/compose.prod.yml up -d
```

## 8. Check The Service

Container status:

```sh
docker compose -f deploy/compose.prod.yml ps
```

Logs:

```sh
docker compose -f deploy/compose.prod.yml logs -f app
docker compose -f deploy/compose.prod.yml logs -f caddy
docker compose -f deploy/compose.prod.yml logs -f redis
```

Health check through HTTPS:

```sh
curl -fsS https://api.example.com/healthz
```

Issue a challenge:

```sh
curl -sS -X POST https://api.example.com/app-attest/challenges \
  -H 'Content-Type: application/json' \
  -d '{"purpose":"attestation","credentialName":"photo_keyid"}' | jq .
```

## 9. Inspect Redis

Enter Redis:

```sh
docker compose -f deploy/compose.prod.yml exec redis redis-cli
```

Useful commands:

```redis
SCAN 0 MATCH tap:* COUNT 100
KEYS tap:challenge:*
KEYS tap:credential:*
GET tap:challenge:<challengeId>
TTL tap:challenge:<challengeId>
GET tap:credential:<keyId>
```

From the shell with JSON formatting:

```sh
docker compose -f deploy/compose.prod.yml exec redis redis-cli GET tap:challenge:<challengeId> | jq .
docker compose -f deploy/compose.prod.yml exec redis redis-cli GET tap:credential:<keyId> | jq .
```

## 10. Update Deployment

For a new image tag:

```sh
docker compose -f deploy/compose.prod.yml pull
docker compose -f deploy/compose.prod.yml up -d
```

For a local rebuild:

```sh
docker build -t tap-app-attest-server:latest .
docker compose -f deploy/compose.prod.yml up -d
```

## 11. Backup Redis

Redis AOF files live in:

```text
/opt/tap-app-attest/data/redis
```

Back up that directory regularly. It contains credential state.
