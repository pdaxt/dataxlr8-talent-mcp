# dataxlr8-talent-mcp

Talent management MCP for DataXLR8 — manage candidates, jobs, applications, and candidate notes with powerful search and submission tracking.

## Tools

| Tool | Description |
|------|-------------|
| add_candidate | Add a new candidate to the talent pool |
| get_candidate | Get candidate details by ID or email |
| list_candidates | List candidates with filters by status, skills, salary range (paginated) |
| update_candidate | Update candidate information |
| delete_candidate | Delete a candidate and all related submissions |
| add_candidate_note | Add a note to a candidate's profile |
| list_candidate_notes | Get all notes for a candidate (paginated) |
| add_job | Add a new open job position |
| get_job | Get job details by ID |
| list_jobs | List jobs with filters by status, location, salary range (paginated) |
| update_job | Update job information |
| delete_job | Delete a job and related submissions |
| submit_candidate | Submit a candidate for a job position |
| get_submission | Get submission details by ID |
| list_submissions | List submissions with filters by status, candidate, or job (paginated) |
| update_submission | Update submission status |
| find_candidates_for_job | Find matching candidates for a job based on skills and experience |
| saved_searches | Search and save candidate search criteria |

## Setup

```bash
DATABASE_URL=postgres://dataxlr8:dataxlr8@localhost:5432/dataxlr8 cargo run
```

## Schema

Creates `talent.*` schema in PostgreSQL with tables for:
- `candidates` — candidate profiles with skills, experience, salary expectations
- `jobs` — open positions with requirements and salary ranges
- `submissions` — candidate applications and submissions to jobs
- `candidate_notes` — notes and feedback on candidates
- `saved_searches` — saved candidate search criteria

## Part of

[DataXLR8](https://github.com/pdaxt) - AI-powered recruitment platform
