# trinity-echo

[![CI](https://github.com/dnacenta/trinity-echo/actions/workflows/ci.yml/badge.svg?branch=development)](https://github.com/dnacenta/trinity-echo/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/github/license/dnacenta/trinity-echo)](LICENSE)
[![Version](https://img.shields.io/github/v/tag/dnacenta/trinity-echo?label=version&color=green)](https://github.com/dnacenta/trinity-echo/tags)
[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange)](https://rustup.rs/)

Voice interface for Claude Code over the phone. Call in and talk to Claude, or trigger outbound calls from n8n / automation workflows.

Built in Rust. Uses Twilio for telephony, Groq Whisper for speech-to-text, ElevenLabs for text-to-speech, and the Claude Code CLI for reasoning.

## Architecture

### Voice Pipeline

```
                         ┌─────────────────────────────────────┐
                         │          trinity-echo (axum)        │
                         │                                     │
  Phone ◄──► Twilio ◄──►│  WebSocket ◄──► VAD ──► STT (Groq)  │
                         │                         │           │
                         │                    Claude CLI        │
                         │                         │           │
                         │                    TTS (ElevenLabs)  │
                         │                         │           │
                         │                    mu-law encode     │
                         └─────────────────────────────────────┘
```

### AI-Initiated Outbound Calls (n8n Bridge)

```
  ┌──────────────┐    trigger     ┌──────────────────┐
  │  Any n8n      │──────────────►│   Orchestrator    │
  │  workflow     │               │   reads registry, │
  │  (alerts,     │               │   routes to       │
  │   cron,       │               │   target module   │
  │   events...) │               └────────┬─────────┘
  └──────────────┘                        │
                                          ▼
                                 ┌──────────────────┐
                                 │   call-human      │
                                 │   builds request, │
                                 │   passes context  │
                                 └────────┬─────────┘
                                          │
                                          │ POST /api/call
                                          │ { to, context }
                                          ▼
                                 ┌──────────────────┐
                                 │  trinity-echo     │
                                 │  stores context   │──► Twilio ──► Phone rings
                                 │  per call_sid     │
                                 └────────┬─────────┘
                                          │
                                          │ caller picks up
                                          ▼
                                 ┌──────────────────┐
                                 │  Claude CLI       │
                                 │  first prompt     │
                                 │  includes context │
                                 │  "I'm calling     │
                                 │   because..."     │
                                 └──────────────────┘
```

### Full System

```
                     ┌──────────┐
  ┌─────────┐        │   n8n    │        ┌───────────────┐
  │ Triggers │──────►│ (Docker) │──────►│ trinity-echo  │──► Claude CLI
  │ (cron,   │       │          │  API   │ (Rust, axum)  │
  │  webhook,│       │  orchest.│        └───────┬───────┘
  │  alerts, │       │  call-   │                │
  │  events) │       │  human   │                ▼
  └─────────┘        └──────────┘        ┌───────────────┐
                                         │    Twilio      │◄──► Phone
                                         └───────────────┘
```

## Prerequisites

- [Rust](https://rustup.rs/) (1.75+)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) installed and authenticated
- [Twilio](https://www.twilio.com/) account with a phone number
- [Groq](https://console.groq.com/) API key (free tier works)
- [ElevenLabs](https://elevenlabs.io/) API key (free tier: ~10k chars/month)
- A server with a public HTTPS URL (for Twilio webhooks)
- nginx (recommended, for TLS termination and WebSocket proxying)

## Installation

### 1. Clone and build

```bash
git clone https://github.com/dnacenta/trinity-echo.git
cd trinity-echo
cargo build --release
```

### 2. Run the setup wizard

```bash
./target/release/trinity-echo --setup
```

The wizard walks you through the entire setup:

- Checks that `rustc`, `claude`, and `openssl` are available
- Prompts for Twilio, Groq, and ElevenLabs credentials (masked input)
- Asks for your server's external URL
- Generates an API token for the outbound call endpoint
- Writes `~/.trinity-echo/config.toml` and `.env` (secrets stored in `.env` with 0600 permissions)
- Optionally copies the binary to `/usr/local/bin/`, installs a systemd service, and generates an nginx reverse proxy config

If you skip the optional steps during the wizard, you can always set them up manually using the templates in `deploy/`.

### 3. Twilio webhook

In the [Twilio Console](https://console.twilio.com/), set your phone number's voice webhook to:

```
POST https://your-server.example.com/twilio/voice
```

### 4. Start

```bash
trinity-echo
```

Or if you installed the systemd service:

```bash
sudo systemctl enable --now trinity-echo
```

### Manual configuration

If you prefer to skip the wizard and configure by hand:

```bash
mkdir -p ~/.trinity-echo
cp config.example.toml ~/.trinity-echo/config.toml
cp .env.example ~/.trinity-echo/.env
chmod 600 ~/.trinity-echo/.env
```

Edit `.env` with your API keys, and `config.toml` for your Twilio phone number and other settings. Secrets are loaded from `.env`, so leave them empty in the TOML. See `deploy/nginx.conf` and `deploy/trinity-echo.service` for server setup templates.

You can override the config directory with `TRINITY_ECHO_CONFIG=/path/to/config.toml`.

## Configuration Reference

### config.toml

| Section       | Field                  | Default                   | Description                                      |
|---------------|------------------------|---------------------------|--------------------------------------------------|
| `server`      | `host`                 | --                        | Bind address (e.g. `0.0.0.0`)                    |
| `server`      | `port`                 | --                        | Bind port (e.g. `8443`)                          |
| `server`      | `external_url`         | --                        | Public HTTPS URL (overridden by `SERVER_EXTERNAL_URL` env var) |
| `twilio`      | `account_sid`          | --                        | Twilio Account SID (overridden by env var)       |
| `twilio`      | `auth_token`           | --                        | Twilio Auth Token (overridden by env var)        |
| `twilio`      | `phone_number`         | --                        | Your Twilio phone number (E.164)                 |
| `groq`        | `api_key`              | --                        | Groq API key (overridden by env var)             |
| `groq`        | `model`                | `whisper-large-v3-turbo`  | Whisper model to use                             |
| `elevenlabs`  | `api_key`              | --                        | ElevenLabs API key (overridden by env var)       |
| `elevenlabs`  | `voice_id`             | `EST9Ui6982FZPSi7gCHi`   | ElevenLabs voice ID                              |
| `claude`      | `session_timeout_secs` | `300`                     | Conversation session timeout                     |
| `api`         | `token`                | --                        | Bearer token for `/api/*` (overridden by env var)|
| `vad`         | `silence_threshold_ms` | `1500`                    | Silence duration before utterance ends           |
| `vad`         | `energy_threshold`     | `50`                      | Minimum RMS energy to detect speech              |

### Environment variables

All secrets can be set via env vars (recommended) instead of config.toml:

| Variable               | Overrides                  |
|------------------------|----------------------------|
| `TWILIO_ACCOUNT_SID`   | `twilio.account_sid`       |
| `TWILIO_AUTH_TOKEN`    | `twilio.auth_token`        |
| `GROQ_API_KEY`         | `groq.api_key`             |
| `ELEVENLABS_API_KEY`   | `elevenlabs.api_key`       |
| `TRINITY_API_TOKEN`   | `api.token`                |
| `SERVER_EXTERNAL_URL`  | `server.external_url`      |
| `TRINITY_ECHO_CONFIG` | Config file path            |
| `RUST_LOG`             | Log level filter            |

## Usage

### Call in

Just call your Twilio number. You'll hear "Connected to Claude. Go ahead and speak." then talk normally.

### Trigger an outbound call

```bash
curl -X POST https://your-server.example.com/api/call \
  -H "Authorization: Bearer YOUR_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "to": "+34612345678",
    "context": "Server CPU at 95% for the last 10 minutes. Top processes: n8n 45%, claude 30%."
  }'
```

The recipient picks up and Claude already knows why it called -- context is injected into the first prompt. The optional `message` field adds a Twilio `<Say>` greeting before the stream starts (usually not needed since Claude handles the greeting via TTS).

### n8n Bridge

trinity-echo integrates with n8n through a bridge architecture:

- **Orchestrator** -- central webhook that routes triggers to registered modules
- **Modules** -- individual workflows managed via a JSON registry
- **call-human** -- module that triggers outbound calls with context

Trigger a call from any n8n workflow via the orchestrator:

```bash
curl -X POST http://localhost:5678/webhook/orchestrator \
  -H "Content-Type: application/json" \
  -H "X-Bridge-Secret: YOUR_BRIDGE_SECRET" \
  -d '{
    "action": "trigger",
    "module": "call-human",
    "data": {
      "reason": "Server CPU critical",
      "context": "CPU at 95% for 10 minutes. Load average 12.5.",
      "urgency": "high"
    }
  }'
```

The orchestrator reads the module registry, forwards the payload to the `call-human` webhook, which calls the trinity-echo API with context. When the user picks up, Claude knows exactly what's happening.

Any n8n workflow can trigger calls by routing through the orchestrator. See `specs/n8n-bridge-spec.md` for the full specification.

## Costs

| Service      | Free tier                     | Paid                             |
|--------------|-------------------------------|----------------------------------|
| Twilio       | Trial credit (~$15)           | ~$1.15/mo number + per-minute    |
| Groq         | Free (rate-limited)           | Usage-based                      |
| ElevenLabs   | ~10k chars/month              | From $5/month                    |
| Claude Code  | Included with Max plan        | Or API usage                     |

For personal use with a few calls a day, the running cost is minimal beyond the Twilio number.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for branch naming, commit conventions, and workflow.

## License

[GPL-3.0](LICENSE)

## Acknowledgments

*Inspired by [NetworkChuck's claude-phone](https://github.com/networkchuck/claude-phone). Rewritten from scratch in Rust with a different architecture -- no intermediate Node.js server, direct WebSocket pipeline, energy-based VAD, and an outbound call API for automation.*
