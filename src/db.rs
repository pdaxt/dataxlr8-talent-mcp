use anyhow::Result;
use sqlx::PgPool;

pub async fn setup_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS talent;

        CREATE TABLE IF NOT EXISTS talent.candidates (
            id               TEXT PRIMARY KEY,
            name             TEXT NOT NULL,
            email            TEXT NOT NULL UNIQUE,
            phone            TEXT NOT NULL DEFAULT '',
            skills           TEXT[] NOT NULL DEFAULT '{}',
            experience_years INTEGER NOT NULL DEFAULT 0,
            current_company  TEXT NOT NULL DEFAULT '',
            desired_salary   DOUBLE PRECISION,
            resume_url       TEXT NOT NULL DEFAULT '',
            source           TEXT NOT NULL DEFAULT '',
            status           TEXT NOT NULL DEFAULT 'sourced'
                             CHECK (status IN ('sourced','screening','interview','offer','placed','rejected')),
            created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS talent.jobs (
            id            TEXT PRIMARY KEY,
            title         TEXT NOT NULL,
            company       TEXT NOT NULL,
            description   TEXT NOT NULL DEFAULT '',
            requirements  TEXT[] NOT NULL DEFAULT '{}',
            salary_min    DOUBLE PRECISION,
            salary_max    DOUBLE PRECISION,
            location      TEXT NOT NULL DEFAULT '',
            status        TEXT NOT NULL DEFAULT 'open'
                          CHECK (status IN ('open','closed','filled','on_hold')),
            created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS talent.submissions (
            id           TEXT PRIMARY KEY,
            candidate_id TEXT NOT NULL REFERENCES talent.candidates(id) ON DELETE CASCADE,
            job_id       TEXT NOT NULL REFERENCES talent.jobs(id) ON DELETE CASCADE,
            submitted_by TEXT NOT NULL DEFAULT '',
            status       TEXT NOT NULL DEFAULT 'submitted'
                         CHECK (status IN ('submitted','reviewing','interview','offered','placed','rejected','withdrawn')),
            submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (candidate_id, job_id)
        );

        CREATE TABLE IF NOT EXISTS talent.candidate_notes (
            id           TEXT PRIMARY KEY,
            candidate_id TEXT NOT NULL REFERENCES talent.candidates(id) ON DELETE CASCADE,
            note         TEXT NOT NULL,
            author       TEXT NOT NULL DEFAULT '',
            created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS talent.saved_searches (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            criteria   JSONB NOT NULL DEFAULT '{}',
            created_by TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX IF NOT EXISTS idx_candidates_email ON talent.candidates(email);
        CREATE INDEX IF NOT EXISTS idx_candidates_status ON talent.candidates(status);
        CREATE INDEX IF NOT EXISTS idx_jobs_status ON talent.jobs(status);
        CREATE INDEX IF NOT EXISTS idx_submissions_candidate ON talent.submissions(candidate_id);
        CREATE INDEX IF NOT EXISTS idx_submissions_job ON talent.submissions(job_id);
        CREATE INDEX IF NOT EXISTS idx_notes_candidate ON talent.candidate_notes(candidate_id);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
