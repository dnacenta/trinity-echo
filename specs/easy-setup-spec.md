# Easy Setup Spec — Docker Compose + Verification Wizard

Version: 1.0
Date: 2026-02-22

---

## Problem

Voice-echo requires five external service accounts, a VPS with HTTPS, and manual wiring of nginx, systemd, and Twilio webhooks. A developer goes from "I found this repo" to "I'm talking to my AI on the phone" in three to eight hours depending on experience. Most of that time is spent on infrastructure, not on voice-echo itself.

The goal is to get that down to twenty minutes.

---

## Solution

Two changes:

1. **Docker Compose** — one `.env` file, one command, full stack running with auto-SSL.
2. **Verification wizard** — replaces the current setup wizard. Runs after deployment, tests every service connection, and guides the user through the one remaining manual step (Twilio webhook).

---

## What Goes Away

The current setup wizard (`--setup`) collects credentials interactively and writes config files. With Docker Compose, the user fills in a single `.env` file and the config is generated from it. The credential collection flow is replaced by editing one file.

The current split between `config.toml` and `.env` goes away for Docker users. Everything lives in `.env`. The container reads env vars directly — no TOML file needed inside the container. The TOML config path remains for bare-metal users who want fine-grained control, but Docker is the primary path.

The nginx config template (`deploy/nginx.conf`) goes away for Docker users. Caddy handles HTTPS automatically inside the compose stack.

---

## Docker Compose Stack

### Services

```yaml
# docker-compose.yml
services:
  voice-echo:
    build: .
    restart: unless-stopped
    environment:
      - TWILIO_ACCOUNT_SID
      - TWILIO_AUTH_TOKEN
      - TWILIO_PHONE_NUMBER
      - GROQ_API_KEY
      - ELEVENLABS_API_KEY
      - ELEVENLABS_VOICE_ID=${ELEVENLABS_VOICE_ID:-EST9Ui6982FZPSi7gCHi}
      - ECHO_API_TOKEN
      - SERVER_EXTERNAL_URL=https://${DOMAIN}
      - RUST_LOG=${RUST_LOG:-voice_echo=info}
      - CLAUDE_GREETING=${CLAUDE_GREETING:-Hello, this is Echo}
      - VAD_SILENCE_MS=${VAD_SILENCE_MS:-1500}
      - VAD_ENERGY=${VAD_ENERGY:-50}
    volumes:
      - claude-data:/root/.claude
      - echo-config:/root/.voice-echo
    networks:
      - internal

  caddy:
    image: caddy:2-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy-data:/data
      - caddy-config:/config
    networks:
      - internal

volumes:
  claude-data:
  echo-config:
  caddy-data:
  caddy-config:

networks:
  internal:
```

### Caddyfile

```
{$DOMAIN} {
    # Twilio webhooks + WebSocket
    reverse_proxy /twilio/* voice-echo:8443

    # Outbound call API
    reverse_proxy /api/* voice-echo:8443

    # Health check
    reverse_proxy /health voice-echo:8443
}
```

Caddy automatically provisions and renews Let's Encrypt certificates. No certbot, no nginx config, no manual cert management.

### .env file

```bash
# === REQUIRED ===

# Your domain (must point to this server)
DOMAIN=ai.example.com

# Twilio — https://console.twilio.com
TWILIO_ACCOUNT_SID=AC...
TWILIO_AUTH_TOKEN=
TWILIO_PHONE_NUMBER=+1...

# Groq (Whisper STT) — https://console.groq.com
GROQ_API_KEY=gsk_...

# ElevenLabs (TTS) — https://elevenlabs.io
ELEVENLABS_API_KEY=

# API token for /api/call endpoint (generate: openssl rand -hex 32)
ECHO_API_TOKEN=

# === OPTIONAL ===

# ELEVENLABS_VOICE_ID=EST9Ui6982FZPSi7gCHi
# CLAUDE_GREETING=Hello, this is Echo
# VAD_SILENCE_MS=1500
# VAD_ENERGY=50
# RUST_LOG=voice_echo=info
```

One file. Every secret and every configurable value in one place. Comments include direct links to where the user gets each key.

### Dockerfile

```dockerfile
FROM rust:1.93-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI
RUN curl -fsSL https://claude.ai/install.sh | sh

COPY --from=builder /app/target/release/voice-echo /usr/local/bin/

EXPOSE 8443

CMD ["voice-echo"]
```

Note: Claude Code CLI needs authentication. The user must run `docker exec -it <container> claude login` once after first deployment, or mount an existing `~/.claude` directory. This is called out in the verification wizard.

---

## Verification Wizard

### New purpose

The wizard no longer collects credentials. It verifies that everything is working after the user has deployed with Docker Compose. It runs as a CLI command:

```bash
docker exec -it voice-echo voice-echo --check
```

Or for bare-metal:

```bash
voice-echo --check
```

### What it checks

**Step 1 — Environment**
- All required env vars present (TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, GROQ_API_KEY, ELEVENLABS_API_KEY, ECHO_API_TOKEN, DOMAIN or SERVER_EXTERNAL_URL)
- Phone number in valid E.164 format
- Print pass/fail for each

**Step 2 — Claude Code CLI**
- `claude` binary in PATH
- Claude is authenticated (run `claude -p "test" --output-format json` with a trivial prompt)
- If not authenticated, print: "Run: claude login"
- Print pass/fail

**Step 3 — Twilio**
- Hit Twilio API with account_sid + auth_token: `GET /2010-04-01/Accounts/{sid}.json`
- Verify the phone number belongs to this account
- Check if voice webhook URL is set on the number — if not, print the exact URL to set and a direct link to the Twilio console page for that number
- Print pass/fail

**Step 4 — Groq**
- Send a tiny audio sample to Groq Whisper API (a one-second silence WAV, bundled as a test fixture)
- Verify response comes back with a transcript (even if empty)
- Print pass/fail

**Step 5 — ElevenLabs**
- Hit ElevenLabs API: `GET /v1/voices` with API key
- Verify the configured voice_id exists in the account
- Print pass/fail

**Step 6 — External URL reachability**
- Hit `https://{DOMAIN}/health` from inside the container
- If unreachable, suggest checking DNS, firewall, and Caddy logs
- Print pass/fail

**Step 7 — Summary**
```
  voice-echo health check

  ✓ Environment variables     all required vars present
  ✓ Claude Code CLI           authenticated, model responding
  ✓ Twilio                    credentials valid, phone number confirmed
  ✗ Twilio webhook            not configured
    → Set voice webhook to: POST https://ai.example.com/twilio/voice
    → Twilio console: https://console.twilio.com/us1/develop/phone-numbers/manage/incoming/{phone_sid}
  ✓ Groq (Whisper)            API responding, STT working
  ✓ ElevenLabs                API responding, voice ID valid
  ✓ External URL              https://ai.example.com/health reachable

  5/6 checks passed. Fix the items above and re-run: voice-echo --check
```

### Twilio webhook automation

The wizard can't set the webhook fully automatically without Twilio API write access, which the user's auth token already provides. Offer it:

```
  Twilio webhook is not configured for this number.

  Set it automatically? [Y/n]
```

If yes, use the Twilio API:
```
POST /2010-04-01/Accounts/{sid}/IncomingPhoneNumbers/{phone_sid}.json
  VoiceUrl=https://{domain}/twilio/voice
  VoiceMethod=POST
```

This eliminates the last manual step.

---

## Updated User Journey

```
1. Clone the repo
   git clone https://github.com/dnacenta/voice-echo.git
   cd voice-echo

2. Copy and fill .env
   cp .env.example .env
   nano .env    # paste your API keys

3. Start the stack
   docker compose up -d

4. Authenticate Claude (first time only)
   docker exec -it voice-echo-voice-echo-1 claude login

5. Run verification
   docker exec -it voice-echo-voice-echo-1 voice-echo --check

6. Call your Twilio number — you're live.
```

Six steps. No nginx. No certbot. No systemd. No TOML files. Twenty minutes if you already have the API accounts.

---

## Config Loading Changes

Currently, config loads from `~/.voice-echo/config.toml` with `.env` overrides. For Docker, we need a mode where everything comes from env vars with sensible defaults and no TOML file required.

### New config resolution order
1. If `ECHO_CONFIG` env var is set, load that TOML file
2. If `~/.voice-echo/config.toml` exists, load it
3. Otherwise, build config entirely from env vars with defaults

### New env vars (additions for Docker)
| Variable | Default | Maps to |
|----------|---------|---------|
| `DOMAIN` | — | `server.external_url` (prefixed with `https://`) |
| `TWILIO_PHONE_NUMBER` | — | `twilio.phone_number` |
| `ELEVENLABS_VOICE_ID` | `EST9Ui6982FZPSi7gCHi` | `elevenlabs.voice_id` |
| `CLAUDE_GREETING` | `Hello, this is Echo` | `claude.greeting` |
| `VAD_SILENCE_MS` | `1500` | `vad.silence_threshold_ms` |
| `VAD_ENERGY` | `50` | `vad.energy_threshold` |

Existing env vars (TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, etc.) continue to work as before. The new vars fill gaps that previously required TOML.

### Bare-metal path preserved

The TOML + .env path still works for users who don't want Docker. The `--setup` flag transitions to `--check` for verification. The old `--setup` behavior (credential collection + file writing) is removed — if a bare-metal user wants to configure manually, they copy `config.example.toml` and `.env.example` as documented in the README.

---

## README Changes

The README gets restructured. Docker Compose becomes the primary installation path at the top. Bare-metal becomes a secondary section for advanced users. The setup wizard section becomes the verification section.

### New structure

```
# voice-echo

[badges, description, architecture diagrams — unchanged]

## Quick Start (Docker)

### Prerequisites
- Docker and Docker Compose
- A server with a public IP and a domain pointed to it
- Accounts: Twilio (with phone number), Groq, ElevenLabs, Anthropic (Claude)

### 1. Clone
  git clone ...
  cd voice-echo

### 2. Configure
  cp .env.example .env
  # Edit .env with your API keys (see comments for where to get each one)

### 3. Deploy
  docker compose up -d

### 4. Authenticate Claude (first time)
  docker exec -it voice-echo-voice-echo-1 claude login

### 5. Verify
  docker exec -it voice-echo-voice-echo-1 voice-echo --check

### 6. Call your number

## Bare-Metal Installation
[existing flow, minus the wizard, plus --check]

## Configuration Reference
[table of all env vars and TOML fields — merged into one reference]

## Usage
[unchanged — call in, trigger outbound, n8n bridge]

## Costs
[unchanged]
```

---

## Pre-built Binaries

Add GitHub Actions release workflow. On tag push, build release binaries for:
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

Attach as release assets. Bare-metal users can download instead of building from source. Cuts the "build from source" step from five minutes to ten seconds.

---

## Implementation Order

1. **Config loading** — add env-var-only mode, new env vars (DOMAIN, TWILIO_PHONE_NUMBER, etc.)
2. **Verification wizard** — new `--check` command replacing `--setup`, all validation steps
3. **Dockerfile** — multi-stage build, Claude CLI installation
4. **Docker Compose + Caddyfile** — full stack definition
5. **README rewrite** — Docker-first, bare-metal secondary
6. **CI release workflow** — pre-built binaries on tag push
7. **Remove old wizard** — delete credential collection code from setup module, keep `--check`

---

## Verification

- [ ] Fresh VPS: clone, fill .env, `docker compose up -d`, authenticate Claude, `--check` passes all steps
- [ ] Twilio webhook auto-configuration works when user approves
- [ ] Bare-metal path still works with TOML + .env
- [ ] Config loads correctly from env vars only (no TOML file present)
- [ ] Pre-built binary downloads and runs on clean Linux
- [ ] `--check` correctly identifies each failure mode (wrong API key, missing env var, unreachable URL, unconfigured webhook)
- [ ] Caddy auto-provisions SSL certificate on first boot
