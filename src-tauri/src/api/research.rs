//! `research.*` handlers: research profile (interests, Google Scholar link), Scholar refresh and import of
//! Scholar articles into Published. See docs/ARCHITECTURE.md ("Research engine").

use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::research::{opportunities, runs::Run, scholar};
use crate::validation::{ensure, Validator};
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "research.profile" => profile(ctx),
        "research.save" => save(args, ctx),
        "research.saveScholar" => save_scholar(args, ctx),
        "research.refreshScholar" => refresh_scholar(ctx),
        "research.scholarArticles" => scholar::last_articles(ctx).map(|l| json!(l)),
        "research.importPublications" => import(args, ctx),
        _ => return None,
    })
}

const SCHOLAR_MESSAGE: &str = "Use your profile link, which starts with https://scholar.google.com/citations?user=…";

fn scholar_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://scholar.google.") else {
        return false;
    };
    let Some((host, _)) = rest.split_once("/citations?") else {
        return false;
    };
    !host.is_empty() && host.chars().all(|c| c.is_ascii_lowercase() || c == '.')
}

fn profile(ctx: &Ctx) -> Result<Value> {
    let u = ctx.require_user()?;
    Ok(json!({
        "scholar_url": u["scholar_url"],
        "interests": u["interests"],
        "suggested": opportunities::interests_for(ctx, None),
        "scholar_stats": u["scholar_stats"],
        "scholar_checked_at": u["scholar_checked_at"],
        "scholar_running": scholar_running(ctx)?,
    }))
}

fn scholar_running(ctx: &Ctx) -> Result<Option<i64>> {
    ctx.db.scalar(
        "SELECT id FROM ai_runs WHERE user_id = ?1 AND kind = ?2 AND status = 'running' ORDER BY id DESC LIMIT 1",
        rusqlite::params![USER_ID, crate::research::runs::SCHOLAR_KIND],
    )
}

fn set_scholar(ctx: &Ctx, url: Option<String>) -> Result<()> {
    let current: Option<String> = ctx.db.scalar("SELECT scholar_url FROM users WHERE id = ?1", [USER_ID])?;
    let mut data = json!({ "scholar_url": url });
    if current != url {
        data["scholar_stats"] = Value::Null;
        data["scholar_checked_at"] = Value::Null;
    }
    ctx.db.update("users", USER_ID, &data)?;
    Ok(())
}

/// Settings → Research "Save": `{ scholar_url?, interests? }`.
fn save(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let v = Validator::new(args)
        .rule("scholar_url", "nullable|url|max:255")
        .rule("interests", "nullable|string|max:1000")
        .message("scholar_url.url", SCHOLAR_MESSAGE)
        .validate()?;
    let url = v.opt("scholar_url");
    if let Some(u) = &url {
        ensure(scholar_ok(u), "scholar_url", SCHOLAR_MESSAGE)?;
    }
    set_scholar(ctx, url)?;
    ctx.db.update("users", USER_ID, &json!({ "interests": v.opt("interests") }))?;
    if let Some(u) = ctx.user()? {
        ctx.emit("user", u);
    }
    profile(ctx)
}

/// Published page "Connect Google Scholar": `{ scholarUrl }` (required).
fn save_scholar(args: &Value, ctx: &Ctx) -> Result<Value> {
    ctx.require_user()?;
    let v = Validator::new(args).rule("scholarUrl", "required|url").message("scholarUrl.url", SCHOLAR_MESSAGE).validate()?;
    let url = v.string("scholarUrl");
    ensure(scholar_ok(&url), "scholarUrl", SCHOLAR_MESSAGE)?;
    set_scholar(ctx, Some(url))?;
    profile(ctx)
}

/// Start "Update from Scholar" → `{ run }` (progress on `ai-run`; notification when done).
fn refresh_scholar(ctx: &Ctx) -> Result<Value> {
    let u = ctx.require_user()?;
    if u["scholar_url"].as_str().is_none_or(str::is_empty) {
        return Err(ApiError::msg("Add your Google Scholar profile link first."));
    }
    if let Some(id) = scholar_running(ctx)? {
        return Ok(json!({ "run": id, "already": true }));
    }
    let run = Run::begin(ctx, crate::research::runs::SCHOLAR_KIND, crate::research::runs::SCHOLAR_LABEL, 1, Some("#/publications"))?;
    let id = run.id;
    ctx.spawn(move |ctx| scholar::refresh_job(ctx, run, false));
    Ok(json!({ "run": id }))
}

/// Add Scholar articles that are not in Published yet: `{ titles?: [..] }` (all when omitted) → `{ added }`.
fn import(args: &Value, ctx: &Ctx) -> Result<Value> {
    let titles: Option<Vec<String>> =
        args.get("titles").and_then(Value::as_array).map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect());
    let added = scholar::import(ctx, titles.as_deref())?;
    Ok(json!({ "added": added }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;

    #[test]
    fn profile_rules() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let e = dispatch("research.save", &json!({ "scholar_url": "https://example.com/x" }), &ctx).unwrap_err();
        assert!(serde_json::to_string(&e).unwrap().contains("starts with https://scholar.google.com/citations?user="));
        ctx.db.update("users", 1, &json!({ "scholar_stats": { "all": {} } })).unwrap();
        let p = dispatch(
            "research.save",
            &json!({ "scholar_url": "https://scholar.google.com/citations?user=abc", "interests": "CSCW" }),
            &ctx,
        )
        .unwrap();
        assert_eq!(p["interests"], "CSCW");
        assert!(p["scholar_stats"].is_null(), "stats reset when the link changes");
        assert!(dispatch("research.saveScholar", &json!({}), &ctx).is_err());
        assert_eq!(dispatch("research.importPublications", &json!({}), &ctx).unwrap()["added"], 0);
    }
}
