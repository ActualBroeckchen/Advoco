//! Applying a FamiliarBlueprint to a running Proto-Familiar instance.
//!
//! Everything goes through PF's local HTTP API so that Phylactery's own MCP
//! tools, snapshotting, and consent gates do the actual writes. Advoco never
//! touches the SQLite store and never computes embeddings.
//!
//! Endpoint contracts verified against Proto-Familiar's server.js:
//! - POST /api/entity/snapshots                (no body) → snapshot id
//! - POST /api/entity/identity                 {category in self|ward|relationship|custom,
//!                                              filename, heading?, content, mode}
//! - PUT  /api/settings                        {settings: {...partial...}} — server merges
//! - POST /api/entity/graph/nodes              {label, type, description}
//! - POST /api/import-logs                     {content, selfNames?, commit, provider, apiKey, model}

use crate::blueprint::FamiliarBlueprint;
use crate::urlguard;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub step: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub ok: bool,
    pub snapshot_id: Option<String>,
    pub steps: Vec<StepResult>,
}

pub struct PfClient {
    base: String,
    http: reqwest::Client,
}

impl PfClient {
    pub fn new(port: u16) -> Self {
        let base = urlguard::pf_base_url(port);
        urlguard::validate_pf_url(&base).expect("fixed PF origin is valid by construction");
        Self {
            base,
            http: reqwest::Client::new(),
        }
    }

    /// Is a Proto-Familiar instance listening and healthy?
    pub async fn detect(&self) -> Result<bool, String> {
        let resp = self
            .http
            .get(format!("{}/api/settings", self.base))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Ok(resp.status().is_success())
    }

    async fn post_json(&self, path: &str, body: Value) -> Result<reqwest::Response, String> {
        self.http
            .post(format!("{}{path}", self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())
    }

    /// (category, filename) pairs already present in the store, for the
    /// identity re-run skip. Empty when the read fails — worst case a
    /// re-run appends again, and a snapshot was taken first anyway.
    async fn existing_identity_files(&self) -> Vec<(String, String)> {
        let Ok(resp) = self
            .http
            .get(format!("{}/api/entity/identity", self.base))
            .send()
            .await
        else {
            return Vec::new();
        };
        let Ok(Value::Object(map)) = resp.json::<Value>().await else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (category, files) in map {
            if let Some(list) = files.as_array() {
                for f in list {
                    if let Some(filename) = f.get("filename").and_then(|f| f.as_str()) {
                        out.push((category.clone(), filename.to_string()));
                    }
                }
            }
        }
        out
    }

    /// The full one-click apply. Order: snapshot → identity → settings →
    /// graph → (optional) chatlog forward. Steps after a hard failure are
    /// skipped and reported.
    pub async fn apply(
        &self,
        bp: &FamiliarBlueprint,
        settings: &crate::blueprint::SettingsPatch,
        chatlog: Option<ChatlogForward<'_>>,
    ) -> ApplyReport {
        let mut steps = Vec::new();
        let mut snapshot_id = None;

        // 1. Safety snapshot — abort if this fails (Phylactery likely down).
        match self.post_json("/api/entity/snapshots", json!({})).await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(v) = resp.json::<Value>().await {
                    snapshot_id = v
                        .get("id")
                        .or_else(|| v.get("snapshotId"))
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string());
                }
                steps.push(StepResult {
                    step: "safety-snapshot".into(),
                    ok: true,
                    detail: "A restore point was saved before anything changed.".into(),
                });
            }
            Ok(resp) => {
                let status = resp.status();
                let err = resp.text().await.unwrap_or_default();
                steps.push(StepResult {
                    step: "safety-snapshot".into(),
                    ok: false,
                    detail: format!("Proto-Familiar answered {status}: {err}"),
                });
                return ApplyReport { ok: false, snapshot_id, steps };
            }
            Err(e) => {
                steps.push(StepResult {
                    step: "safety-snapshot".into(),
                    ok: false,
                    detail: format!("Could not reach Proto-Familiar: {e}"),
                });
                return ApplyReport { ok: false, snapshot_id, steps };
            }
        }

        // 2. Identity files. Primary: mode update_section (the intended
        // wiring, fixed upstream 2026-08-16). Because the pre-fix failure
        // mode was a SILENT ok:true, every write is verified by re-reading
        // the store; anything that didn't land falls back to append mode
        // (which works on all builds) with the heading embedded. Files that
        // already exist are left untouched so re-runs never clobber an
        // existing Familiar's identity.
        let existing = self.existing_identity_files().await;
        let writes = bp.render_identity_writes();
        let mut pending: Vec<&crate::blueprint::IdentityWrite> = Vec::new();
        for w in &writes {
            let step = format!("identity/{}/{}", w.category, w.filename);
            if existing.iter().any(|(c, f)| c == w.category && f == w.filename) {
                steps.push(StepResult {
                    step,
                    ok: true,
                    detail: "already present — left untouched (re-run skipped it)".into(),
                });
                continue;
            }
            let body = json!({
                "category": w.category,
                "filename": w.filename,
                "heading":  w.heading,
                "content":  w.content,
                "mode":     "update_section",
            });
            match self.post_json("/api/entity/identity", body).await {
                Ok(resp) if resp.status().is_success() => pending.push(w),
                Ok(resp) => {
                    let status = resp.status();
                    let err = resp.text().await.unwrap_or_default();
                    steps.push(StepResult {
                        step,
                        ok: false,
                        detail: format!("{status}: {err}"),
                    });
                }
                Err(e) => steps.push(StepResult {
                    step,
                    ok: false,
                    detail: e.to_string(),
                }),
            }
        }

        // 2b. Verify the writes landed; append-fallback for anything that
        // didn't (pre-fix Proto-Familiar builds swallow update_section).
        if !pending.is_empty() {
            let after = self.existing_identity_files().await;
            for w in pending {
                let step = format!("identity/{}/{}", w.category, w.filename);
                let landed = after.iter().any(|(c, f)| c == w.category && f == w.filename);
                if landed {
                    steps.push(StepResult {
                        step,
                        ok: true,
                        detail: "written".into(),
                    });
                    continue;
                }
                let fallback = json!({
                    "category": w.category,
                    "filename": w.filename,
                    "content":  format!("## {}\n\n{}\n", w.heading, w.content),
                    "mode":     "append",
                });
                match self.post_json("/api/entity/identity", fallback).await {
                    Ok(resp) if resp.status().is_success() => steps.push(StepResult {
                        step,
                        ok: true,
                        detail: "written (compatibility mode — consider updating Proto-Familiar)".into(),
                    }),
                    Ok(resp) => {
                        let status = resp.status();
                        let err = resp.text().await.unwrap_or_default();
                        steps.push(StepResult {
                            step,
                            ok: false,
                            detail: format!("write did not land and fallback failed ({status}): {err}"),
                        });
                    }
                    Err(e) => steps.push(StepResult {
                        step,
                        ok: false,
                        detail: e.to_string(),
                    }),
                }
            }
        }

        // 3. Settings patch (server merges with prior — safe to send partial).
        let mut patch = serde_json::Map::new();
        if let Some(v) = &settings.char_name {
            patch.insert("charName".into(), json!(v));
        }
        if let Some(v) = &settings.user_name {
            patch.insert("userName".into(), json!(v));
        }
        if let Some(v) = &settings.character_profile {
            patch.insert("characterProfile".into(), json!(v));
        }
        if let Some(v) = &settings.user_profile {
            patch.insert("userProfile".into(), json!(v));
        }
        if let Some(v) = &settings.post_history_prompt {
            patch.insert("postHistoryPrompt".into(), json!(v));
        }
        if !patch.is_empty() {
            match self
                .http
                .put(format!("{}/api/settings", self.base))
                .json(&json!({ "settings": Value::Object(patch) }))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => steps.push(StepResult {
                    step: "settings".into(),
                    ok: true,
                    detail: "name, profiles, and the always-there reminder were set.".into(),
                }),
                Ok(resp) => {
                    let status = resp.status();
                    let err = resp.text().await.unwrap_or_default();
                    steps.push(StepResult {
                        step: "settings".into(),
                        ok: false,
                        detail: format!("{status}: {err}"),
                    });
                }
                Err(e) => steps.push(StepResult {
                    step: "settings".into(),
                    ok: false,
                    detail: e.to_string(),
                }),
            }
        }

        // 4. Graph nodes (best effort — never blocks the apply).
        for g in &bp.graph_entities {
            let body = json!({
                "label": g.label,
                "type": g.node_type,
                "description": g.description,
            });
            match self.post_json("/api/entity/graph/nodes", body).await {
                Ok(resp) if resp.status().is_success() => steps.push(StepResult {
                    step: format!("graph/{}", g.label),
                    ok: true,
                    detail: "node created".into(),
                }),
                Ok(resp) => {
                    let status = resp.status();
                    let err = resp.text().await.unwrap_or_default();
                    steps.push(StepResult {
                        step: format!("graph/{}", g.label),
                        ok: false,
                        detail: format!("{status}: {err}"),
                    });
                }
                Err(e) => steps.push(StepResult {
                    step: format!("graph/{}", g.label),
                    ok: false,
                    detail: e.to_string(),
                }),
            }
        }

        // 5. Optional chatlog forward through PF's own consent-gated pipeline.
        if let Some(fwd) = chatlog {
            steps.push(self.forward_chatlog_inner(fwd).await);
        }

        let ok = steps.iter().filter(|s| !s.ok && !s.step.starts_with("graph/")).count() == 0
            && steps.first().map(|s| s.ok).unwrap_or(false);
        ApplyReport { ok, snapshot_id, steps }
    }

    /// Forward a foreign chatlog into PF's import pipeline (preview, then
    /// commit using the ward's own LLM config, which never leaves the machine).
    pub async fn forward_chatlog(&self, fwd: ChatlogForward<'_>) -> StepResult {
        self.forward_chatlog_inner(fwd).await
    }

    async fn forward_chatlog_inner(&self, fwd: ChatlogForward<'_>) -> StepResult {
        let step_name = "chatlog-import".to_string();
        let mut body = json!({ "content": fwd.content, "commit": false });
        if let Some(names) = fwd.self_names.clone() {
            body["selfNames"] = json!(names);
        }
        // Preview first — surfaces format errors without writing anything.
        let preview = match self.post_json("/api/import-logs", body).await {
            Ok(resp) if resp.status().is_success() => resp.json::<Value>().await,
            Ok(resp) => {
                let status = resp.status();
                let err = resp.text().await.unwrap_or_default();
                return StepResult {
                    step: step_name,
                    ok: false,
                    detail: format!("Proto-Familiar could not read the log ({status}): {err}"),
                };
            }
            Err(e) => {
                return StepResult {
                    step: step_name,
                    ok: false,
                    detail: e.to_string(),
                }
            }
        };
        let pv = match preview {
            Ok(v) => v,
            Err(e) => {
                return StepResult {
                    step: step_name,
                    ok: false,
                    detail: format!("unreadable preview reply: {e}"),
                }
            }
        };
        if pv.get("needsDate").and_then(|d| d.as_bool()).unwrap_or(false) {
            return StepResult {
                step: step_name,
                ok: false,
                detail: "This log has no timestamps and needs a date picked for it — \
not supported by Advoco yet; import it from Proto-Familiar's own UI."
                    .into(),
            };
        }
        let Some((provider, api_key, model)) = fwd.llm else {
            return StepResult {
                step: step_name,
                ok: false,
                detail: "chatlog import needs an LLM connection for Proto-Familiar to \
memorize with — set one up on Advoco's connection screen first."
                    .into(),
            };
        };
        let mut commit_body = json!({
            "content": fwd.content,
            "commit": true,
            "provider": provider,
            "apiKey": api_key,
            "model": model,
        });
        if let Some(names) = fwd.self_names.clone() {
            commit_body["selfNames"] = json!(names);
        }
        match self.post_json("/api/import-logs", commit_body).await {
            Ok(resp) if resp.status().is_success() => {
                let days = resp
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("days").and_then(|d| d.as_i64()))
                    .unwrap_or(0);
                StepResult {
                    step: step_name,
                    ok: true,
                    detail: format!(
                        "{days} day(s) of conversation handed to Proto-Familiar — \
your Familiar will remember them, with your consent settings applied."
                    ),
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let err = resp.text().await.unwrap_or_default();
                StepResult {
                    step: step_name,
                    ok: false,
                    detail: format!("{status}: {err}"),
                }
            }
            Err(e) => StepResult {
                step: step_name,
                ok: false,
                detail: e,
            },
        }
    }
}

/// What the ward asked to forward, plus the local LLM credentials PF itself
/// will use for ingestion (they go over loopback to the ward's own PF only).
pub struct ChatlogForward<'a> {
    pub content: &'a str,
    /// Names the ward appears under in the log, e.g. their chosen userName.
    pub self_names: Option<Vec<String>>,
    /// (provider-key, api-key, model) if an LLM connection is configured.
    pub llm: Option<(String, String, String)>,
}
