use dataxlr8_mcp_core::mcp::{empty_schema, error_result, get_f64, get_i64, get_str, get_str_array, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const DEFAULT_OFFSET: i64 = 0;

const VALID_CANDIDATE_STATUSES: &[&str] = &["sourced", "screening", "interview", "offer", "placed", "rejected"];
const VALID_JOB_STATUSES: &[&str] = &["open", "closed", "filled", "on_hold"];

// ============================================================================
// Validation helpers
// ============================================================================

/// Trim a string, returning None if empty after trimming
fn trim_non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Get a required trimmed string param, or return an error message
fn require_trimmed_str(args: &serde_json::Value, key: &str) -> Result<String, String> {
    match get_str(args, key) {
        Some(s) => trim_non_empty(&s)
            .ok_or_else(|| format!("Parameter '{}' cannot be empty or whitespace-only", key)),
        None => Err(format!("Missing required parameter: {}", key)),
    }
}

/// Get an optional trimmed string param
fn optional_trimmed_str(args: &serde_json::Value, key: &str) -> Option<String> {
    get_str(args, key).and_then(|s| trim_non_empty(&s))
}

/// Basic email validation (must contain @ with text on both sides)
fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.')
}

/// Validate limit/offset pagination params and clamp to sane defaults
fn pagination(args: &serde_json::Value) -> (i64, i64) {
    let limit = get_i64(args, "limit")
        .unwrap_or(DEFAULT_LIMIT)
        .max(1)
        .min(MAX_LIMIT);
    let offset = get_i64(args, "offset")
        .unwrap_or(DEFAULT_OFFSET)
        .max(0);
    (limit, offset)
}

/// Trim all strings in an array and filter out empty entries
fn trimmed_str_array(args: &serde_json::Value, key: &str) -> Vec<String> {
    get_str_array(args, key)
        .into_iter()
        .filter_map(|s| trim_non_empty(&s))
        .collect()
}

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub skills: Vec<String>,
    pub experience_years: i32,
    pub current_company: String,
    pub desired_salary: Option<f64>,
    pub resume_url: String,
    pub source: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: String,
    pub title: String,
    pub company: String,
    pub description: String,
    pub requirements: Vec<String>,
    pub salary_min: Option<f64>,
    pub salary_max: Option<f64>,
    pub location: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Submission {
    pub id: String,
    pub candidate_id: String,
    pub job_id: String,
    pub submitted_by: String,
    pub status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CandidateNote {
    pub id: String,
    pub candidate_id: String,
    pub note: String,
    pub author: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedSearch {
    pub id: String,
    pub name: String,
    pub criteria: serde_json::Value,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct MatchResult {
    pub candidate: Candidate,
    pub fit_score: f64,
    pub matching_skills: Vec<String>,
    pub missing_skills: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PipelineEntry {
    pub candidate_id: String,
    pub candidate_name: String,
    pub candidate_email: String,
    pub submission_status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PlacementRow {
    pub submitted_by: String,
    pub placements: i64,
    pub avg_days_to_fill: Option<f64>,
}

// ============================================================================
// Tool definitions
// ============================================================================

/// Pagination properties shared across list/search tools
fn pagination_props() -> serde_json::Value {
    serde_json::json!({
        "limit": { "type": "integer", "description": "Max results to return (default: 50, max: 200)" },
        "offset": { "type": "integer", "description": "Number of results to skip for pagination (default: 0)" }
    })
}

/// Merge two JSON objects
fn merge_props(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
    let mut map = match a {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(m2) = b {
        for (k, v) in m2 {
            map.insert(k, v);
        }
    }
    serde_json::Value::Object(map)
}

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "add_candidate".into(),
            title: None,
            description: Some("Add a new candidate to the talent pool".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "name": { "type": "string", "description": "Full name" },
                    "email": { "type": "string", "description": "Email address" },
                    "phone": { "type": "string", "description": "Phone number" },
                    "skills": { "type": "array", "items": { "type": "string" }, "description": "List of skills" },
                    "experience_years": { "type": "integer", "description": "Years of experience (must be >= 0)" },
                    "current_company": { "type": "string", "description": "Current employer" },
                    "desired_salary": { "type": "number", "description": "Desired annual salary (must be > 0)" },
                    "resume_url": { "type": "string", "description": "URL to resume/CV" },
                    "source": { "type": "string", "description": "Where the candidate was sourced from (e.g. LinkedIn, referral)" }
                }),
                vec!["name", "email"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "search_candidates".into(),
            title: None,
            description: Some("Search candidates with full-text search on name, skills, company. Filter by min experience, skills match, salary range. Supports pagination with limit/offset.".into()),
            input_schema: make_schema(
                merge_props(
                    serde_json::json!({
                        "query": { "type": "string", "description": "Free-text search across name, skills, company" },
                        "min_experience": { "type": "integer", "description": "Minimum years of experience (must be >= 0)" },
                        "skills": { "type": "array", "items": { "type": "string" }, "description": "Required skills (candidates must have ALL)" },
                        "salary_min": { "type": "number", "description": "Minimum desired salary (must be > 0)" },
                        "salary_max": { "type": "number", "description": "Maximum desired salary (must be > 0)" },
                        "status": { "type": "string", "description": "Filter by candidate status" }
                    }),
                    pagination_props(),
                ),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "create_job".into(),
            title: None,
            description: Some("Create a new job opening".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "title": { "type": "string", "description": "Job title" },
                    "company": { "type": "string", "description": "Hiring company" },
                    "description": { "type": "string", "description": "Job description" },
                    "requirements": { "type": "array", "items": { "type": "string" }, "description": "Required skills/qualifications" },
                    "salary_min": { "type": "number", "description": "Minimum salary (must be > 0)" },
                    "salary_max": { "type": "number", "description": "Maximum salary (must be > 0)" },
                    "location": { "type": "string", "description": "Job location" },
                    "status": { "type": "string", "enum": ["open", "closed", "filled", "on_hold"], "description": "Job status (default: open)" }
                }),
                vec!["title", "company"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "match_candidates".into(),
            title: None,
            description: Some("Find candidates matching a job's requirements, ranked by fit score based on skill overlap and experience. Supports pagination with limit/offset.".into()),
            input_schema: make_schema(
                merge_props(
                    serde_json::json!({
                        "job_id": { "type": "string", "description": "Job ID to match against" }
                    }),
                    pagination_props(),
                ),
                vec!["job_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "submit_candidate".into(),
            title: None,
            description: Some("Submit a candidate to a job opening, tracking submission status".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "candidate_id": { "type": "string", "description": "Candidate ID" },
                    "job_id": { "type": "string", "description": "Job ID" },
                    "submitted_by": { "type": "string", "description": "Recruiter name/ID who submitted" }
                }),
                vec!["candidate_id", "job_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "update_status".into(),
            title: None,
            description: Some("Move a candidate through recruitment stages: sourced, screening, interview, offer, placed, rejected. Updates candidate status and submission status if job_id provided.".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "candidate_id": { "type": "string", "description": "Candidate ID" },
                    "status": { "type": "string", "enum": ["sourced", "screening", "interview", "offer", "placed", "rejected"], "description": "New stage" },
                    "job_id": { "type": "string", "description": "If provided, also update the submission status for this job" }
                }),
                vec!["candidate_id", "status"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "candidate_pipeline".into(),
            title: None,
            description: Some("Show all candidates by stage for a specific job. Supports pagination with limit/offset.".into()),
            input_schema: make_schema(
                merge_props(
                    serde_json::json!({
                        "job_id": { "type": "string", "description": "Job ID to view pipeline for" }
                    }),
                    pagination_props(),
                ),
                vec!["job_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "placement_stats".into(),
            title: None,
            description: Some("Get placement statistics: placements by recruiter, average time to fill, conversion rates".into()),
            input_schema: empty_schema(),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "add_note".into(),
            title: None,
            description: Some("Add a recruiter note to a candidate".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "candidate_id": { "type": "string", "description": "Candidate ID" },
                    "note": { "type": "string", "description": "Note content" },
                    "author": { "type": "string", "description": "Author name" }
                }),
                vec!["candidate_id", "note"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "talent_search_saved".into(),
            title: None,
            description: Some("Save or list saved search criteria for reuse. If 'name' and 'criteria' provided, saves a new search. Otherwise lists all saved searches with pagination.".into()),
            input_schema: make_schema(
                merge_props(
                    serde_json::json!({
                        "name": { "type": "string", "description": "Name for the saved search" },
                        "criteria": { "type": "object", "description": "Search criteria to save (query, skills, min_experience, salary_min, salary_max, status)" },
                        "created_by": { "type": "string", "description": "Who created this saved search" }
                    }),
                    pagination_props(),
                ),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
    ]
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct TalentMcpServer {
    db: Database,
}

impl TalentMcpServer {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // ---- Tool handlers ----

    async fn handle_add_candidate(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match require_trimmed_str(args, "name") {
            Ok(n) => n,
            Err(e) => return error_result(&e),
        };
        let email = match require_trimmed_str(args, "email") {
            Ok(e) => e,
            Err(e) => return error_result(&e),
        };
        if !is_valid_email(&email) {
            return error_result("Invalid email format: must contain '@' with a valid domain (e.g. user@example.com)");
        }

        let phone = optional_trimmed_str(args, "phone").unwrap_or_default();
        let skills = trimmed_str_array(args, "skills");
        let experience_years = get_i64(args, "experience_years").unwrap_or(0) as i32;
        if experience_years < 0 {
            return error_result("experience_years must be >= 0");
        }
        let current_company = optional_trimmed_str(args, "current_company").unwrap_or_default();
        let desired_salary = get_f64(args, "desired_salary");
        if let Some(salary) = desired_salary {
            if salary <= 0.0 {
                return error_result("desired_salary must be greater than 0");
            }
        }
        let resume_url = optional_trimmed_str(args, "resume_url").unwrap_or_default();
        let source = optional_trimmed_str(args, "source").unwrap_or_default();

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Candidate>(
            r#"INSERT INTO talent.candidates (id, name, email, phone, skills, experience_years, current_company, desired_salary, resume_url, source)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING *"#,
        )
        .bind(&id)
        .bind(&name)
        .bind(&email)
        .bind(&phone)
        .bind(&skills)
        .bind(experience_years)
        .bind(&current_company)
        .bind(desired_salary)
        .bind(&resume_url)
        .bind(&source)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(c) => {
                info!(name = %name, email = %email, id = %id, "Added candidate");
                json_result(&c)
            }
            Err(e) => {
                error!(error = %e, name = %name, email = %email, "Failed to add candidate");
                error_result(&format!("Failed to add candidate: {e}"))
            }
        }
    }

    async fn handle_search_candidates(&self, args: &serde_json::Value) -> CallToolResult {
        let query = optional_trimmed_str(args, "query");
        let min_experience = get_i64(args, "min_experience").map(|v| v as i32);
        if let Some(min_exp) = min_experience {
            if min_exp < 0 {
                return error_result("min_experience must be >= 0");
            }
        }
        let skills = trimmed_str_array(args, "skills");
        let salary_min = get_f64(args, "salary_min");
        let salary_max = get_f64(args, "salary_max");
        if let Some(smin) = salary_min {
            if smin <= 0.0 {
                return error_result("salary_min must be greater than 0");
            }
        }
        if let Some(smax) = salary_max {
            if smax <= 0.0 {
                return error_result("salary_max must be greater than 0");
            }
        }
        if let (Some(smin), Some(smax)) = (salary_min, salary_max) {
            if smin > smax {
                return error_result("salary_min cannot be greater than salary_max");
            }
        }
        let status = optional_trimmed_str(args, "status");
        if let Some(ref st) = status {
            if !VALID_CANDIDATE_STATUSES.contains(&st.as_str()) {
                return error_result(&format!(
                    "Invalid status '{}'. Must be one of: {}",
                    st,
                    VALID_CANDIDATE_STATUSES.join(", ")
                ));
            }
        }
        let (limit, offset) = pagination(args);

        // Build dynamic query
        let mut sql = String::from("SELECT * FROM talent.candidates WHERE 1=1");
        let mut param_idx = 1u32;

        struct Params {
            strings: Vec<String>,
            ints: Vec<i32>,
            floats: Vec<f64>,
            arrays: Vec<Vec<String>>,
            binds: Vec<BindType>,
        }
        enum BindType {
            Str(usize),
            Int(usize),
            Float(usize),
            StrArray(usize),
        }
        let mut params = Params {
            strings: Vec::new(),
            ints: Vec::new(),
            floats: Vec::new(),
            arrays: Vec::new(),
            binds: Vec::new(),
        };

        if let Some(ref q) = query {
            sql.push_str(&format!(
                " AND (name ILIKE ${p} OR current_company ILIKE ${p} OR array_to_string(skills, ' ') ILIKE ${p})",
                p = param_idx
            ));
            params.strings.push(format!("%{q}%"));
            params.binds.push(BindType::Str(params.strings.len() - 1));
            param_idx += 1;
        }

        if let Some(min_exp) = min_experience {
            sql.push_str(&format!(" AND experience_years >= ${}", param_idx));
            params.ints.push(min_exp);
            params.binds.push(BindType::Int(params.ints.len() - 1));
            param_idx += 1;
        }

        if !skills.is_empty() {
            sql.push_str(&format!(" AND skills @> ${}", param_idx));
            params.arrays.push(skills);
            params.binds.push(BindType::StrArray(params.arrays.len() - 1));
            param_idx += 1;
        }

        if let Some(smin) = salary_min {
            sql.push_str(&format!(" AND desired_salary >= ${}", param_idx));
            params.floats.push(smin);
            params.binds.push(BindType::Float(params.floats.len() - 1));
            param_idx += 1;
        }

        if let Some(smax) = salary_max {
            sql.push_str(&format!(" AND desired_salary <= ${}", param_idx));
            params.floats.push(smax);
            params.binds.push(BindType::Float(params.floats.len() - 1));
            param_idx += 1;
        }

        if let Some(ref st) = status {
            sql.push_str(&format!(" AND status = ${}", param_idx));
            params.strings.push(st.clone());
            params.binds.push(BindType::Str(params.strings.len() - 1));
            param_idx += 1;
        }

        sql.push_str(&format!(" ORDER BY created_at DESC LIMIT ${} OFFSET ${}", param_idx, param_idx + 1));
        // Bind limit and offset as i64
        params.floats.push(limit as f64); // placeholder — we'll bind as i64 below
        params.floats.push(offset as f64);

        // Actually we need to bind limit/offset differently. Let's use ints.
        // Remove the float placeholders we just pushed.
        params.floats.pop();
        params.floats.pop();

        // Build the query with dynamic bindings
        let mut q = sqlx::query_as::<_, Candidate>(&sql);
        for bind in &params.binds {
            match bind {
                BindType::Str(i) => q = q.bind(&params.strings[*i]),
                BindType::Int(i) => q = q.bind(params.ints[*i]),
                BindType::Float(i) => q = q.bind(params.floats[*i]),
                BindType::StrArray(i) => q = q.bind(&params.arrays[*i]),
            }
        }
        // Bind limit and offset
        q = q.bind(limit);
        q = q.bind(offset);

        match q.fetch_all(self.db.pool()).await {
            Ok(candidates) => json_result(&candidates),
            Err(e) => {
                error!(error = %e, "Search candidates failed");
                error_result(&format!("Search failed: {e}"))
            }
        }
    }

    async fn handle_create_job(&self, args: &serde_json::Value) -> CallToolResult {
        let title = match require_trimmed_str(args, "title") {
            Ok(t) => t,
            Err(e) => return error_result(&e),
        };
        let company = match require_trimmed_str(args, "company") {
            Ok(c) => c,
            Err(e) => return error_result(&e),
        };
        let description = optional_trimmed_str(args, "description").unwrap_or_default();
        let requirements = trimmed_str_array(args, "requirements");
        let salary_min = get_f64(args, "salary_min");
        let salary_max = get_f64(args, "salary_max");
        if let Some(smin) = salary_min {
            if smin <= 0.0 {
                return error_result("salary_min must be greater than 0");
            }
        }
        if let Some(smax) = salary_max {
            if smax <= 0.0 {
                return error_result("salary_max must be greater than 0");
            }
        }
        if let (Some(smin), Some(smax)) = (salary_min, salary_max) {
            if smin > smax {
                return error_result("salary_min cannot be greater than salary_max");
            }
        }
        let location = optional_trimmed_str(args, "location").unwrap_or_default();
        let status = optional_trimmed_str(args, "status").unwrap_or_else(|| "open".into());

        if !VALID_JOB_STATUSES.contains(&status.as_str()) {
            return error_result(&format!(
                "Invalid status '{}'. Must be one of: {}",
                status,
                VALID_JOB_STATUSES.join(", ")
            ));
        }

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Job>(
            r#"INSERT INTO talent.jobs (id, title, company, description, requirements, salary_min, salary_max, location, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING *"#,
        )
        .bind(&id)
        .bind(&title)
        .bind(&company)
        .bind(&description)
        .bind(&requirements)
        .bind(salary_min)
        .bind(salary_max)
        .bind(&location)
        .bind(&status)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(job) => {
                info!(title = %title, company = %company, id = %id, "Created job");
                json_result(&job)
            }
            Err(e) => {
                error!(error = %e, title = %title, company = %company, "Failed to create job");
                error_result(&format!("Failed to create job: {e}"))
            }
        }
    }

    async fn handle_match_candidates(&self, args: &serde_json::Value) -> CallToolResult {
        let job_id = match require_trimmed_str(args, "job_id") {
            Ok(j) => j,
            Err(e) => return error_result(&e),
        };
        let (limit, offset) = pagination(args);

        // Fetch the job to get requirements
        let job: Job = match sqlx::query_as("SELECT * FROM talent.jobs WHERE id = $1")
            .bind(&job_id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(j)) => j,
            Ok(None) => return error_result(&format!("Job '{}' not found", job_id)),
            Err(e) => {
                error!(error = %e, job_id = %job_id, "Failed to fetch job for matching");
                return error_result(&format!("Database error: {e}"));
            }
        };

        if job.requirements.is_empty() {
            return error_result("Job has no requirements defined — cannot match");
        }

        // Fetch candidates who have at least one overlapping skill
        let candidates: Vec<Candidate> = match sqlx::query_as::<_, Candidate>(
            "SELECT * FROM talent.candidates WHERE skills && $1 AND status NOT IN ('placed', 'rejected') ORDER BY experience_years DESC",
        )
        .bind(&job.requirements)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, job_id = %job_id, "Match query failed");
                return error_result(&format!("Match query failed: {e}"));
            }
        };

        let req_set: std::collections::HashSet<String> =
            job.requirements.iter().map(|s| s.to_lowercase()).collect();
        let req_count = req_set.len() as f64;

        let mut results: Vec<MatchResult> = candidates
            .into_iter()
            .map(|c| {
                let cand_skills: std::collections::HashSet<String> =
                    c.skills.iter().map(|s| s.to_lowercase()).collect();
                let matching: Vec<String> = req_set.intersection(&cand_skills).cloned().collect();
                let missing: Vec<String> = req_set.difference(&cand_skills).cloned().collect();
                let skill_score = matching.len() as f64 / req_count;
                let exp_bonus = (c.experience_years as f64 / 20.0).min(0.2);
                let fit_score = ((skill_score * 0.8 + exp_bonus) * 100.0).round() / 100.0;
                MatchResult {
                    candidate: c,
                    fit_score,
                    matching_skills: matching,
                    missing_skills: missing,
                }
            })
            .collect();

        results.sort_by(|a, b| b.fit_score.partial_cmp(&a.fit_score).unwrap_or(std::cmp::Ordering::Equal));

        // Apply pagination: skip `offset` results, take `limit`
        let paginated: Vec<MatchResult> = results
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        json_result(&paginated)
    }

    async fn handle_submit_candidate(&self, args: &serde_json::Value) -> CallToolResult {
        let candidate_id = match require_trimmed_str(args, "candidate_id") {
            Ok(c) => c,
            Err(e) => return error_result(&e),
        };
        let job_id = match require_trimmed_str(args, "job_id") {
            Ok(j) => j,
            Err(e) => return error_result(&e),
        };
        let submitted_by = optional_trimmed_str(args, "submitted_by").unwrap_or_default();

        // Verify candidate exists
        match sqlx::query_as::<_, Candidate>("SELECT * FROM talent.candidates WHERE id = $1")
            .bind(&candidate_id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return error_result(&format!("Candidate '{}' not found", candidate_id)),
            Err(e) => {
                error!(error = %e, candidate_id = %candidate_id, "Failed to verify candidate");
                return error_result(&format!("Database error: {e}"));
            }
        }

        // Verify job exists
        match sqlx::query_as::<_, Job>("SELECT * FROM talent.jobs WHERE id = $1")
            .bind(&job_id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return error_result(&format!("Job '{}' not found", job_id)),
            Err(e) => {
                error!(error = %e, job_id = %job_id, "Failed to verify job");
                return error_result(&format!("Database error: {e}"));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Submission>(
            r#"INSERT INTO talent.submissions (id, candidate_id, job_id, submitted_by)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(&id)
        .bind(&candidate_id)
        .bind(&job_id)
        .bind(&submitted_by)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(sub) => {
                info!(candidate_id = %candidate_id, job_id = %job_id, id = %id, "Submitted candidate to job");
                json_result(&sub)
            }
            Err(e) => {
                error!(error = %e, candidate_id = %candidate_id, job_id = %job_id, "Failed to submit candidate");
                error_result(&format!("Failed to submit candidate: {e}"))
            }
        }
    }

    async fn handle_update_status(&self, args: &serde_json::Value) -> CallToolResult {
        let candidate_id = match require_trimmed_str(args, "candidate_id") {
            Ok(c) => c,
            Err(e) => return error_result(&e),
        };
        let status = match require_trimmed_str(args, "status") {
            Ok(s) => s,
            Err(e) => return error_result(&e),
        };
        let job_id = optional_trimmed_str(args, "job_id");

        if !VALID_CANDIDATE_STATUSES.contains(&status.as_str()) {
            return error_result(&format!(
                "Invalid status '{}'. Must be one of: {}",
                status,
                VALID_CANDIDATE_STATUSES.join(", ")
            ));
        }

        // Update candidate status
        match sqlx::query_as::<_, Candidate>(
            "UPDATE talent.candidates SET status = $1 WHERE id = $2 RETURNING *",
        )
        .bind(&status)
        .bind(&candidate_id)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(Some(c)) => {
                info!(candidate_id = %candidate_id, status = %status, "Updated candidate status");

                // Also update submission status if job_id provided
                if let Some(ref jid) = job_id {
                    let sub_status = match status.as_str() {
                        "screening" => "reviewing",
                        "interview" => "interview",
                        "offer" => "offered",
                        "placed" => "placed",
                        "rejected" => "rejected",
                        _ => "submitted",
                    };
                    if let Err(e) = sqlx::query(
                        "UPDATE talent.submissions SET status = $1, updated_at = now() WHERE candidate_id = $2 AND job_id = $3",
                    )
                    .bind(sub_status)
                    .bind(&candidate_id)
                    .bind(jid)
                    .execute(self.db.pool())
                    .await
                    {
                        error!(error = %e, candidate_id = %candidate_id, job_id = %jid, "Failed to update submission status");
                        // Don't fail the whole operation — candidate status was updated
                    }
                }

                json_result(&c)
            }
            Ok(None) => error_result(&format!("Candidate '{}' not found", candidate_id)),
            Err(e) => {
                error!(error = %e, candidate_id = %candidate_id, "Failed to update candidate status");
                error_result(&format!("Failed to update status: {e}"))
            }
        }
    }

    async fn handle_candidate_pipeline(&self, args: &serde_json::Value) -> CallToolResult {
        let job_id = match require_trimmed_str(args, "job_id") {
            Ok(j) => j,
            Err(e) => return error_result(&e),
        };
        let (limit, offset) = pagination(args);

        let entries: Vec<PipelineEntry> = match sqlx::query_as(
            r#"SELECT s.candidate_id, c.name AS candidate_name, c.email AS candidate_email,
                      s.status AS submission_status, s.submitted_at
               FROM talent.submissions s
               JOIN talent.candidates c ON c.id = s.candidate_id
               WHERE s.job_id = $1
               ORDER BY s.status, s.submitted_at
               LIMIT $2 OFFSET $3"#,
        )
        .bind(&job_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(e) => e,
            Err(e) => {
                error!(error = %e, job_id = %job_id, "Pipeline query failed");
                return error_result(&format!("Pipeline query failed: {e}"));
            }
        };

        // Group by stage
        let mut stages: std::collections::BTreeMap<String, Vec<&PipelineEntry>> =
            std::collections::BTreeMap::new();
        for entry in &entries {
            stages
                .entry(entry.submission_status.clone())
                .or_default()
                .push(entry);
        }

        let summary = serde_json::json!({
            "job_id": job_id,
            "total_candidates": entries.len(),
            "limit": limit,
            "offset": offset,
            "by_stage": stages.iter().map(|(k, v)| {
                (k.clone(), serde_json::json!({
                    "count": v.len(),
                    "candidates": v.iter().map(|e| serde_json::json!({
                        "candidate_id": e.candidate_id,
                        "name": e.candidate_name,
                        "email": e.candidate_email,
                        "submitted_at": e.submitted_at,
                    })).collect::<Vec<_>>()
                }))
            }).collect::<serde_json::Map<String, serde_json::Value>>(),
        });

        json_result(&summary)
    }

    async fn handle_placement_stats(&self) -> CallToolResult {
        // Placements by recruiter with avg days to fill
        let recruiter_stats: Vec<PlacementRow> = match sqlx::query_as(
            r#"SELECT submitted_by,
                      COUNT(*) AS placements,
                      AVG(EXTRACT(EPOCH FROM (updated_at - submitted_at)) / 86400.0) AS avg_days_to_fill
               FROM talent.submissions
               WHERE status = 'placed'
               GROUP BY submitted_by
               ORDER BY placements DESC"#,
        )
        .fetch_all(self.db.pool())
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "Recruiter stats query failed");
                return error_result(&format!("Stats query failed: {e}"));
            }
        };

        // Overall conversion rates
        let totals = sqlx::query_as::<_, (i64, i64, i64, i64)>(
            r#"SELECT
                COUNT(*) AS total_submissions,
                COUNT(*) FILTER (WHERE status = 'interview') AS interviews,
                COUNT(*) FILTER (WHERE status = 'offered') AS offers,
                COUNT(*) FILTER (WHERE status = 'placed') AS placements
               FROM talent.submissions"#,
        )
        .fetch_one(self.db.pool())
        .await;

        let conversion = match totals {
            Ok((total, interviews, offers, placements)) => {
                let total_f = total as f64;
                serde_json::json!({
                    "total_submissions": total,
                    "interviews": interviews,
                    "offers": offers,
                    "placements": placements,
                    "submission_to_interview_rate": if total > 0 { format!("{:.1}%", interviews as f64 / total_f * 100.0) } else { "N/A".into() },
                    "interview_to_offer_rate": if interviews > 0 { format!("{:.1}%", offers as f64 / interviews as f64 * 100.0) } else { "N/A".into() },
                    "offer_to_placement_rate": if offers > 0 { format!("{:.1}%", placements as f64 / offers as f64 * 100.0) } else { "N/A".into() },
                })
            }
            Err(e) => {
                error!(error = %e, "Failed to get conversion stats");
                serde_json::json!({ "error": format!("{e}") })
            }
        };

        json_result(&serde_json::json!({
            "by_recruiter": recruiter_stats,
            "conversion_rates": conversion,
        }))
    }

    async fn handle_add_note(&self, args: &serde_json::Value) -> CallToolResult {
        let candidate_id = match require_trimmed_str(args, "candidate_id") {
            Ok(c) => c,
            Err(e) => return error_result(&e),
        };
        let note = match require_trimmed_str(args, "note") {
            Ok(n) => n,
            Err(e) => return error_result(&e),
        };
        let author = optional_trimmed_str(args, "author").unwrap_or_default();

        // Verify candidate exists
        match sqlx::query_as::<_, Candidate>("SELECT * FROM talent.candidates WHERE id = $1")
            .bind(&candidate_id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => return error_result(&format!("Candidate '{}' not found", candidate_id)),
            Err(e) => {
                error!(error = %e, candidate_id = %candidate_id, "Failed to verify candidate for note");
                return error_result(&format!("Database error: {e}"));
            }
        }

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, CandidateNote>(
            r#"INSERT INTO talent.candidate_notes (id, candidate_id, note, author)
               VALUES ($1, $2, $3, $4)
               RETURNING *"#,
        )
        .bind(&id)
        .bind(&candidate_id)
        .bind(&note)
        .bind(&author)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(n) => {
                info!(candidate_id = %candidate_id, id = %id, "Added note");
                json_result(&n)
            }
            Err(e) => {
                error!(error = %e, candidate_id = %candidate_id, "Failed to add note");
                error_result(&format!("Failed to add note: {e}"))
            }
        }
    }

    async fn handle_talent_search_saved(&self, args: &serde_json::Value) -> CallToolResult {
        let name = optional_trimmed_str(args, "name");
        let criteria = args.get("criteria");

        // If name + criteria provided, save. Otherwise list.
        if let (Some(name), Some(criteria)) = (name, criteria) {
            let created_by = optional_trimmed_str(args, "created_by").unwrap_or_default();
            let id = uuid::Uuid::new_v4().to_string();

            match sqlx::query_as::<_, SavedSearch>(
                r#"INSERT INTO talent.saved_searches (id, name, criteria, created_by)
                   VALUES ($1, $2, $3, $4)
                   RETURNING *"#,
            )
            .bind(&id)
            .bind(&name)
            .bind(criteria)
            .bind(&created_by)
            .fetch_one(self.db.pool())
            .await
            {
                Ok(s) => {
                    info!(name = %name, id = %id, "Saved search");
                    json_result(&s)
                }
                Err(e) => {
                    error!(error = %e, name = %name, "Failed to save search");
                    error_result(&format!("Failed to save search: {e}"))
                }
            }
        } else {
            let (limit, offset) = pagination(args);
            // List all saved searches with pagination
            match sqlx::query_as::<_, SavedSearch>(
                "SELECT * FROM talent.saved_searches ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(searches) => json_result(&searches),
                Err(e) => {
                    error!(error = %e, "Failed to list saved searches");
                    error_result(&format!("Failed to list saved searches: {e}"))
                }
            }
        }
    }
}

// ============================================================================
// ServerHandler trait implementation
// ============================================================================

impl ServerHandler for TalentMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "DataXLR8 Talent MCP — recruiter toolkit: candidates, jobs, matching, pipeline tracking, placement stats"
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        async {
            Ok(ListToolsResult {
                tools: build_tools(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let args =
                serde_json::to_value(&request.arguments).unwrap_or(serde_json::Value::Null);
            let name_str: &str = request.name.as_ref();

            let result = match name_str {
                "add_candidate" => self.handle_add_candidate(&args).await,
                "search_candidates" => self.handle_search_candidates(&args).await,
                "create_job" => self.handle_create_job(&args).await,
                "match_candidates" => self.handle_match_candidates(&args).await,
                "submit_candidate" => self.handle_submit_candidate(&args).await,
                "update_status" => self.handle_update_status(&args).await,
                "candidate_pipeline" => self.handle_candidate_pipeline(&args).await,
                "placement_stats" => self.handle_placement_stats().await,
                "add_note" => self.handle_add_note(&args).await,
                "talent_search_saved" => self.handle_talent_search_saved(&args).await,
                _ => error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
