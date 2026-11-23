//! `ai.*` handlers: provider settings (keys in the OS keychain, never returned), connection test, Claude Code
//! detection, research options, and AI runs (list, status, cancel, clear). See docs/ARCHITECTURE.md ("Research engine").

use crate::api::Ctx;
use crate::db::USER_ID;
use crate::error::{ApiError, Result};
use crate::models::{ai_run, setting};
use crate::research::{self, providers, runs};
use crate::validation::{ensure, Validator};
use serde_json::{json, Value};

pub fn handle(name: &str, args: &Value, ctx: &Ctx) -> Option<Result<Value>> {
    Some(match name {
        "ai.settings" => settings(ctx),
        "ai.save" => save(args, ctx),
        "ai.forget" => forget(args, ctx),
        "ai.test" => test(ctx),
        "ai.detectClaude" => detect_claude(ctx),
        "ai.status" => Ok(status(ctx)),
        "ai.runs" => list_runs(args, ctx),
        "ai.running" => running(ctx),
        "ai.run" => run(args, ctx),
        "ai.cancel" => cancel(args, ctx),
        "ai.clear" => clear(ctx),
        _ => return None,
    })
}

fn status(ctx: &Ctx) -> Value {
    json!({
        "driver": providers::driver(&ctx.db),
        "status": providers::status(&ctx.db),
        "ready": providers::configured(&ctx.db),
        "web_search": providers::supports_web(&ctx.db),
    })
}

fn claude_info(ctx: &Ctx, with_version: bool) -> Value {
    let candidates = providers::claude_candidates(&ctx.db);
    let bin = candidates.first().map(|(p, _)| p.clone());
    let version = if with_version { bin.as_deref().and_then(providers::claude_version) } else { None };
    json!({
        "bin": bin.as_ref().map(|p| p.display().to_string()),
        "found_in": candidates.first().map(|(_, w)| *w),
        "version": version,
        "candidates": candidates.iter().map(|(p, w)| json!({ "path": p.display().to_string(), "where": w })).collect::<Vec<_>>(),
        "token_saved": setting::is_set(&ctx.db, "ai.claude_token"),
        "setup_token_command": "claude setup-token",
        "help": "If you use Claude Code in Terminal, the app uses the same sign-in. Otherwise run “claude setup-token” in Terminal and paste the token here.",
    })
}

/// Everything the Settings → AI page needs. Secrets are masked.
fn settings(ctx: &Ctx) -> Result<Value> {
    let providers_list: Vec<Value> = providers::PROVIDERS
        .iter()
        .map(|(key, label)| {
            let (secret, model_key, about, placeholder) = providers::detail(key);
            json!({
                "key": key, "label": label, "about": about, "secret_key": secret, "model_key": model_key,
                "model": setting::get_str(&ctx.db, model_key), "placeholder": placeholder,
                "saved": setting::masked(&ctx.db, secret),
            })
        })
        .collect();
    let mut out = status(ctx);
    out["providers"] = json!(providers_list);
    out["claude_bin"] = json!(setting::get(&ctx.db, "ai.claude_bin").unwrap_or_default());
    out["base_url"] = json!(setting::get(&ctx.db, "compatible.base_url").unwrap_or_default());
    out["claude"] = claude_info(ctx, false);
    out["autofill_empty"] = json!(research::flag(&ctx.db, "research.autofill_empty"));
    out["web_fallback"] = json!(research::flag(&ctx.db, "research.web_fallback"));
    Ok(out)
}

/// Save provider, models, new keys, CLI path, base URL and research options (Settings → AI "Save").
fn save(args: &Value, ctx: &Ctx) -> Result<Value> {
    let keys = crate::models::keys(providers::PROVIDERS);
    let v = Validator::new(args)
        .rule("driver", &format!("required|in:{keys}"))
        .rule("models", "nullable|array")
        .rule("models.*", "nullable|string|max:120")
        .rule("secrets", "nullable|array")
        .rule("secrets.*", "nullable|string|max:400")
        .rule("claudeBin", "nullable|string|max:500")
        .rule("baseUrl", "nullable|url|max:300")
        .rule("autofill_empty", "nullable|boolean")
        .rule("web_fallback", "nullable|boolean")
        .validate()?;
    let secrets = args.get("secrets").and_then(Value::as_object).cloned().unwrap_or_default();
    for (k, val) in &secrets {
        let s = val.as_str().unwrap_or("").trim();
        ensure(
            s.is_empty() || s.chars().all(|c| c.is_ascii_alphanumeric() || "_-.:".contains(c)),
            &format!("secrets.{k}"),
            "Paste the key as one line, with no spaces or quotes.",
        )?;
    }
    let driver = v.string("driver");
    setting::put(&ctx.db, "ai.driver", Some(&driver))?;
    let models = args.get("models").and_then(Value::as_object).cloned().unwrap_or_default();
    for (key, _) in providers::PROVIDERS {
        if let Some(m) = models.get(*key) {
            let m = m.as_str().unwrap_or("").trim();
            setting::put(&ctx.db, providers::detail(key).1, (!m.is_empty()).then_some(m))?;
        }
    }
    for (provider, val) in &secrets {
        let s = val.as_str().unwrap_or("").trim();
        if !s.is_empty() && providers::PROVIDERS.iter().any(|(k, _)| k == provider) {
            setting::put(&ctx.db, providers::detail(provider).0, Some(s))?;
        }
    }
    if args.get("claudeBin").is_some() {
        let b = v.string("claudeBin");
        setting::put(&ctx.db, "ai.claude_bin", (!b.is_empty()).then_some(b.as_str()))?;
    }
    if args.get("baseUrl").is_some() {
        let b = v.string("baseUrl");
        setting::put(&ctx.db, "compatible.base_url", (!b.is_empty()).then_some(b.as_str()))?;
    }
    for k in ["autofill_empty", "web_fallback"] {
        if args.get(k).is_some_and(|x| !x.is_null()) {
            research::set_flag(&ctx.db, &format!("research.{k}"), v.bool(k))?;
        }
    }
    settings(ctx)
}

/// Remove a saved key: `{ key }` (one of the secret setting keys).
fn forget(args: &Value, ctx: &Ctx) -> Result<Value> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("");
    if !setting::is_secret(key) {
        return Err(ApiError::msg("Unknown key."));
    }
    setting::put(&ctx.db, key, None)?;
    settings(ctx)
}

/// A tiny request to the chosen provider: `{ ok, message }`.
fn test(ctx: &Ctx) -> Result<Value> {
    let st = providers::status(&ctx.db);
    Ok(match ctx.block_on(providers::ping(&ctx.db)) {
        Ok(reply) => {
            let reply: String = reply.chars().take(40).collect();
            json!({ "ok": true, "message": format!("Connected to {st}. Reply: “{reply}”") })
        }
        Err(e) => json!({ "ok": false, "message": e.to_string() }),
    })
}

fn detect_claude(ctx: &Ctx) -> Result<Value> {
    Ok(claude_info(ctx, true))
}

fn decorate(mut r: Value) -> Value {
    let pct = ai_run::percent(&r);
    let id = r["id"].as_i64().unwrap_or(0);
    if let Some(o) = r.as_object_mut() {
        o.insert("percent".into(), json!(pct));
        o.insert("cancellable".into(), json!(runs::is_active(id)));
    }
    r
}

/// `{ status?: "running"|"done"|"failed", kind?, limit? }` → rows, newest first (log omitted; see `ai.run`).
fn list_runs(args: &Value, ctx: &Ctx) -> Result<Value> {
    ai_run::close_stale(ctx)?;
    let limit = args.get("limit").and_then(Value::as_i64).unwrap_or(60).clamp(1, 200);
    let status = args.get("status").and_then(Value::as_str).unwrap_or("");
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("");
    let rows = ctx.db.rows(
        "SELECT id, user_id, kind, label, status, total, done, step, url, error, finished_at, created_at, updated_at, provider,
                input_tokens, output_tokens, duration_ms, log
         FROM ai_runs WHERE user_id = ?1 AND (?2 = '' OR status = ?2) AND (?3 = '' OR kind = ?3) ORDER BY id DESC LIMIT ?4",
        rusqlite::params![USER_ID, status, kind, limit],
    )?;
    Ok(json!(rows.into_iter().map(decorate).collect::<Vec<_>>()))
}

/// Running runs (the "AI at work" panel).
fn running(ctx: &Ctx) -> Result<Value> {
    ai_run::close_stale(ctx)?;
    let rows = ctx.db.rows("SELECT * FROM ai_runs WHERE user_id = ?1 AND status = 'running' ORDER BY id DESC", [USER_ID])?;
    Ok(json!(rows.into_iter().map(decorate).collect::<Vec<_>>()))
}

fn run_id(args: &Value) -> Result<i64> {
    args.get("id").and_then(Value::as_i64).ok_or_else(|| ApiError::field("id", "The id field is required."))
}

/// One run with its log and the proposals it created.
fn run(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = run_id(args)?;
    let row = ctx.db.find("ai_runs", id)?.ok_or_else(ApiError::not_found)?;
    let mut row = decorate(row);
    let props: Vec<Value> = ctx
        .db
        .rows("SELECT * FROM proposals WHERE run_id = ?1 ORDER BY id", [id])?
        .into_iter()
        .map(|p| research::proposals::decorate(&ctx.db, p))
        .collect();
    row["proposals"] = json!(props);
    Ok(row)
}

/// Cancel a running run: `{ id }` → `{ cancelled }`. A run not active in this app (stale) is closed.
fn cancel(args: &Value, ctx: &Ctx) -> Result<Value> {
    let id = run_id(args)?;
    if runs::cancel(id) {
        return Ok(json!({ "cancelled": true }));
    }
    let n = ctx.db.exec(
        "UPDATE ai_runs SET status = 'failed', error = ?1, finished_at = ?2, updated_at = ?2 WHERE id = ?3 AND status = 'running'",
        rusqlite::params![runs::CANCELLED, crate::util::now_ts(), id],
    )?;
    if n > 0 {
        if let Some(r) = ctx.db.find("ai_runs", id)? {
            ctx.emit("ai-runs", r);
        }
    }
    Ok(json!({ "cancelled": n > 0 }))
}

/// Delete finished runs (history). Running ones stay.
fn clear(ctx: &Ctx) -> Result<Value> {
    let n = ctx.db.exec("DELETE FROM ai_runs WHERE user_id = ?1 AND status != 'running'", [USER_ID])?;
    ctx.changed("ai_runs");
    Ok(json!({ "deleted": n }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dispatch;

    #[test]
    fn settings_roundtrip() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let err = dispatch("ai.save", &json!({ "driver": "openai", "secrets": { "openai": "sk bad key" } }), &ctx).unwrap_err();
        assert!(serde_json::to_string(&err).unwrap().contains("Paste the key as one line"));
        let s = dispatch("ai.save", &json!({ "driver": "openai", "models": { "openai": "gpt-5-mini" }, "secrets": { "openai": "sk-test-abcdefgh1234" }, "autofill_empty": false }), &ctx).unwrap();
        assert_eq!(s["driver"], "openai");
        assert_eq!(s["status"], "OpenAI (gpt-5-mini)");
        assert_eq!(s["autofill_empty"], false);
        let openai = s["providers"].as_array().unwrap().iter().find(|p| p["key"] == "openai").unwrap().clone();
        assert_eq!(openai["saved"], "sk-…1234");
        assert!(!serde_json::to_string(&s).unwrap().contains("abcdefgh1234"), "secret never returned");
        let s = dispatch("ai.forget", &json!({ "key": "openai.key" }), &ctx).unwrap();
        assert_eq!(s["ready"], false);
        assert!(dispatch("ai.forget", &json!({ "key": "site.name" }), &ctx).is_err());
    }

    #[test]
    fn runs_list_cancel_clear() {
        let ctx = Ctx::for_tests(crate::db::test_db());
        let r = runs::Run::begin(&ctx, "scan", "Calls for papers scan", 3, None).unwrap();
        let list = dispatch("ai.running", &json!({}), &ctx).unwrap();
        assert_eq!(list[0]["cancellable"], true);
        assert_eq!(dispatch("ai.cancel", &json!({ "id": r.id }), &ctx).unwrap()["cancelled"], true);
        r.finish(&ctx, Some("x"), None);
        assert_eq!(dispatch("ai.run", &json!({ "id": r.id }), &ctx).unwrap()["error"], runs::CANCELLED);
        assert_eq!(dispatch("ai.clear", &json!({}), &ctx).unwrap()["deleted"], 1);
    }
}
