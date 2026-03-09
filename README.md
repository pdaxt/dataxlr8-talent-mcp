# :briefcase: dataxlr8-talent-mcp

Recruitment and talent management for AI agents — candidates, jobs, submissions, matching, and saved searches.

[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-rmcp_0.17-blue)](https://modelcontextprotocol.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## What It Does

Manages the full recruitment lifecycle through MCP tool calls. Maintain a candidate pool with skills and salary expectations, post open positions, submit candidates to jobs with status tracking, and use skill-based matching to find the right fit. Supports paginated listing, saved search criteria, and candidate notes — all backed by PostgreSQL.

## Architecture

```
                    ┌─────────────────────────┐
AI Agent ──stdio──▶ │  dataxlr8-talent-mcp    │
                    │  (rmcp 0.17 server)      │
                    └──────────┬──────────────┘
                               │ sqlx 0.8
                               ▼
                    ┌─────────────────────────┐
                    │  PostgreSQL              │
                    │  schema: talent          │
                    │  ├── candidates          │
                    │  ├── jobs                │
                    │  ├── submissions         │
                    │  ├── candidate_notes     │
                    │  └── saved_searches      │
                    └─────────────────────────┘
```

## Tools

| Tool | Description |
|------|-------------|
| `add_candidate` | Add a new candidate to the talent pool |
| `search_candidates` | Search by skills, experience, salary range |
| `update_status` | Update a candidate's pipeline status |
| `add_note` | Add a note to a candidate's profile |
| `create_job` | Create a new open position |
| `submit_candidate` | Submit a candidate for a specific job |
| `match_candidates` | Find matching candidates for a job by skills and experience |
| `candidate_pipeline` | View candidates grouped by pipeline stage |
| `placement_stats` | Get placement and submission statistics |
| `talent_search_saved` | Save and retrieve search criteria |

## Quick Start

```bash
git clone https://github.com/pdaxt/dataxlr8-talent-mcp
cd dataxlr8-talent-mcp
cargo build --release

export DATABASE_URL=postgres://user:pass@localhost:5432/dataxlr8
./target/release/dataxlr8-talent-mcp
```

The server auto-creates the `talent` schema and all tables on first run.

## Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `LOG_LEVEL` | No | Tracing level (default: `info`) |

## Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dataxlr8-talent": {
      "command": "./target/release/dataxlr8-talent-mcp",
      "env": {
        "DATABASE_URL": "postgres://user:pass@localhost:5432/dataxlr8"
      }
    }
  }
}
```

## Part of DataXLR8

One of 14 Rust MCP servers that form the [DataXLR8](https://github.com/pdaxt) platform — a modular, AI-native business operations suite. Each server owns a single domain, shares a PostgreSQL instance, and communicates over the Model Context Protocol.

## License

MIT
