# Pull and publish the TAPNap website

`tap` runs on the ECS host. It downloads a successful GitHub Actions build of
TAPCamVerifier and switches the static website served by Nginx. ECS does not
compile the frontend, and GitHub does not receive an ECS SSH key.

```text
TAPCamVerifier main -> GitHub tests and build -> versioned dist artifact
ECS: tap update web -> download + verify -> current release -> host Nginx
```

This first version implements `update web`, `status web` and `rollback web`.
Backend/Redis operations remain in `scripts/server.sh`; this tool does not yet
replace them or manage HTTPS. ACR continues building the backend independently.

## Install once

The host needs Linux, Python **3.9 or newer** and an existing Nginx installation.
No Python packages, Node, Rust, Git checkout or GitHub CLI are required on ECS.
The backend image does not install this host command; install the script once
before running `tap`. The server repository is private, so the one-time copy
below also avoids needing repository-download credentials on ECS.
Run website updates as an ordinary deployment account without sudo or Docker
access. Use sudo only for installing the tool and configuring Nginx.

From the server checkout on your development computer, copy the tool and Nginx
snippet to that account:

```sh
scp deploy/tap deploy/nginx-web.conf YOUR_USER@ECS_IP:/tmp/
```

On ECS, as that ordinary account:

```sh
python3 --version
sudo install -d -m 0755 /opt/tapnap
sudo install -m 0755 /tmp/tap /opt/tapnap/tap
sudo install -m 0644 /tmp/nginx-web.conf /opt/tapnap/nginx-web.conf
sudo ln -s /opt/tapnap/tap /usr/local/bin/tap
sudo install -d -m 0755 -o "$(id -un)" -g "$(id -gn)" /var/www/tapnap
tap status web
```

If `tap` is already installed, update `/opt/tapnap/tap` with `install` and keep
the existing symlink. Website releases do not modify the installed tool.

## Produce the first artifact

Push the frontend workflow changes to `TAP-NAP/TAPCamVerifier` and wait for
**Deploy GitHub Pages** to finish successfully on `main`. It must include
the artifact upload step; older workflow runs cannot be deployed with this tool.
The website's Rust and JavaScript tests and production build run in GitHub.

The artifact is named `tapnap-web-COMMIT_SHA-RUN_ATTEMPT` and retained by GitHub
for 30 days. The existing overseas Pages deployment continues. A failure of
the workflow, including its Pages job, makes that run ineligible for ECS.

Create a GitHub fine-grained token limited to `TAP-NAP/TAPCamVerifier`, with
**Actions: Read-only** (and the automatically included metadata access).
Authorize it for the organization if required. No repository write or ECS
permission is needed. Enter it in the ECS Bash session without putting it in
shell history, command arguments, source files or the website:

```sh
read -rsp 'GitHub read-only token: ' GH_TOKEN
export GH_TOKEN
printf '\n'
```

The environment variable lasts for this shell session. The tool also accepts `GITHUB_TOKEN`.
It never prints credentials or the temporary signed artifact download URL.

## Test downloads before publishing

The default selects the latest successful `main` build and prints its commit.
You do not need to look up or enter a run ID:

```sh
tap update web --dry-run
```

This fetches metadata, downloads the ZIP, checks its SHA-256 against GitHub,
and extracts it into a temporary directory.
It checks all three HTML entry points and a versioned verifier WASM. Temporary
files are removed and the website directory is not modified.

This is the connectivity test for **both** `api.github.com` and GitHub's
redirected artifact storage. Source-code downloads working on ECS do not prove
artifact downloads work. The tool verifies HTTPS and has a 30-second network
operation timeout. A failed check leaves the website intact; retry after
correcting connectivity, token permissions or an expired artifact.

## Publish and configure Nginx

```sh
tap update web
tap status web
```

The run must be completed and successful, from this repository's `main` branch,
triggered by a push or manual dispatch of `deploy-pages.yml`. Its commit,
attempt and artifact origin are checked together; a concurrent rerun is rejected.

Copy the contents of `/opt/tapnap/nginx-web.conf` into your existing `www.tapnap.net`
`server` block, replacing its old static `location /` definition. Keep the
existing certificate configuration, `/.well-known/acme-challenge/` handling,
and API proxy locations for `/tapcam/` and `/app-attest/`. Then run:

```sh
sudo nginx -t
sudo systemctl reload nginx
curl --resolve www.tapnap.net:443:ECS_IP -I https://www.tapnap.net/
curl --resolve www.tapnap.net:443:ECS_IP -I https://www.tapnap.net/verify/
curl --resolve www.tapnap.net:443:ECS_IP -I https://www.tapnap.net/privacy/
```

Replace `ECS_IP` with the server address. These checks assume HTTPS is already
configured and bypass DNS/CDN to test the origin. Then repeat against the normal
public URLs to test the CDN path. Set HTML to revalidate/no cache
and preserve long caching for `/assets/`; verification APIs must remain uncached.
The tool does not change CDN settings, purge CDN caches or manage certificates.

For later releases, either select an explicit successful run again or use:

```sh
tap update web
```

Without `--run`, the tool selects the **latest successful main workflow run**,
prints its exact commit and run ID, then downloads it. This may be older than
the latest source commit if a newer run is still building or has failed. Use
`--dry-run` to inspect the selection without publishing. Each invocation selects
independently. To pin the same build across checking and publishing, pass the
printed run number as `--run RUN_ID`; this is optional. Tests are not rerun on ECS.

## Roll back

```sh
tap rollback web
tap status web
```

Rollback switches only to the locally retained previous website. It does not
download, rebuild, modify Redis or touch backend containers. Repeating rollback
stays on that previous version; it does not toggle forward. To deploy a newer
version again, use `update web` or select its run explicitly.

## Files, permissions and limits

```text
/opt/tapnap/tap                 installed tool (root-owned)
/var/www/tapnap/
  current -> releases/...      active static site
  previous -> releases/...     rollback target
  releases/                    complete sites and local build metadata
  shared/assets/               retained hashed JS, CSS, images and WASM
```

Nginx needs read/traverse access, while only the deployment account should write
the website tree. Keep tokens, HTTPS keys and backend configuration outside it.
`--root /another/path` is supported for isolated tests; adjust both paths in the
Nginx snippet if changing the real website root.

Downloads are capped at 128 MiB; extraction at 512 MiB and 10,000 entries.
Unsafe paths, symlinks, duplicate ZIP entries, encrypted entries, missing files
and mismatched hashes stop deployment. No downloaded code or shell scripts run
on ECS. GitHub and the trusted repository/CI remain part of the trust boundary;
checksum verification does not certify that a website's content is harmless.

Updates and rollback share a lock. New files are ready before the final atomic
`current` symlink replacement. Failed download, validation or preparation keeps
the active site. An interrupted preparation may leave an unused release or
extra hashed resources, but does not delete an active site's resources.

Hashed assets are shared using hard links, so keeping release directories does
not duplicate those assets. They are deliberately not pruned automatically:
old open tabs and cached HTML may still request them. Monitor disk space with
`tap status web`; choose a retention policy once actual release volume is known.
This retention protection applies to this ECS Nginx layout, not GitHub Pages.

The local checks do not prove live Nginx routing, CDN caching, backend behavior
or ECS connectivity. Validate these on the target host before switching DNS.

## Local regression check

```sh
python3 -B -m unittest discover -s deploy -p 'test_*.py' -v
```

The tests use temporary files and fake GitHub responses; they do not log in to
GitHub, publish a website or operate Docker/Nginx.
