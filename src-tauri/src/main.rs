//! Advoco — onboarding wizard for Proto-Familiar Familiars.
//! Window + command layer; the interesting parts live in the sibling modules.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apply;
mod blueprint;
mod doctrine;
mod export;
mod llm;
mod urlguard;

use blueprint::FamiliarBlueprint;
use llm::{GenerationInput, LlmConfig, Provider};
use serde::Serialize;
use tauri::Manager;

// ---------------------------------------------------------------------------
// App config (non-secret) — stored next to nothing else in the config dir.
// ---------------------------------------------------------------------------

fn config_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("advoco-config.json"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppConfigView {
    llm: LlmConfig,
    has_api_key: bool,
    pf_port: u16,
}

#[tauri::command]
fn get_config(app: tauri::AppHandle) -> Result<AppConfigView, String> {
    let mut cfg: StoredConfig = Default::default();
    if let Ok(text) = std::fs::read_to_string(config_path(&app)?) {
        if let Ok(parsed) = serde_json::from_str(&text) {
            cfg = parsed;
        }
    }
    Ok(AppConfigView {
        has_api_key: llm::load_api_key(&cfg.llm)?.is_some(),
        llm: cfg.llm,
        pf_port: cfg.pf_port,
    })
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    llm: LlmConfig,
    #[serde(default = "default_pf_port")]
    pf_port: u16,
}

fn default_pf_port() -> u16 {
    8742
}

#[tauri::command]
fn set_config(
    app: tauri::AppHandle,
    llm: LlmConfig,
    pf_port: u16,
    api_key: Option<String>,
) -> Result<(), String> {
    if let Some(key) = api_key {
        if !key.trim().is_empty() {
            llm::store_api_key(&llm, key.trim())?;
        }
    }
    let stored = StoredConfig { llm, pf_port };
    std::fs::write(
        config_path(&app)?,
        serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Commands the wizard calls
// ---------------------------------------------------------------------------

#[tauri::command]
async fn test_llm(llm: LlmConfig) -> Result<String, String> {
    llm::test_connection(&llm)
        .await
        .map(|_| "connected".into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn detect_pf(pf_port: u16) -> Result<bool, String> {
    apply::PfClient::new(pf_port).detect().await
}

#[tauri::command]
async fn generate(input: GenerationInput, llm: LlmConfig) -> Result<GenerateOutcome, String> {
    let bp = llm::generate_blueprint(&llm, &input)
        .await
        .map_err(|e| e.to_string())?;
    let checklist = blueprint::run_checklist(&bp);
    Ok(GenerateOutcome {
        blueprint: bp,
        checklist_passed: checklist.passed,
        checklist_failures: checklist.failures,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateOutcome {
    blueprint: FamiliarBlueprint,
    checklist_passed: bool,
    checklist_failures: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyArgs {
    blueprint: FamiliarBlueprint,
    /// The ward's own name, if they gave one.
    user_name: Option<String>,
    pf_port: u16,
    /// { content, selfName } when the ward opted into forwarding.
    chatlog: Option<ChatlogArg>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatlogArg {
    content: String,
    #[serde(default)]
    self_name: Option<String>,
}

#[tauri::command]
async fn apply_blueprint(args: ApplyArgs, llm: LlmConfig) -> Result<apply::ApplyReport, String> {
    // Doctrine gate: never template a blueprint that fails the checklist.
    let checklist = blueprint::run_checklist(&args.blueprint);
    if !checklist.passed {
        return Err(format!(
            "The blueprint does not pass Advoco's own quality rules yet: {}",
            checklist.failures.join("; ")
        ));
    }
    let settings = args.blueprint.render_settings(args.user_name.clone());
    let chatlog_content = args.chatlog.as_ref().map(|c| c.content.clone());
    let chatlog_self_name = args.chatlog.and_then(|c| c.self_name);
    let chatlog = chatlog_content.as_deref().map(|content| {
        let llm_creds = llm_forward_creds(&llm);
        let self_names = chatlog_self_name.clone().map(|n| vec![n]);
        apply::ChatlogForward {
            content,
            self_names,
            llm: llm_creds,
        }
    });
    let client = apply::PfClient::new(args.pf_port);
    Ok(client.apply(&args.blueprint, &settings, chatlog).await)
}

/// Map Advoco's provider config to what PF's import pipeline accepts.
/// PF resolves the ingestion LLM via its PROVIDER_URLS map, which only knows
/// nanogpt / zai / zai-coding / google — anything else can't be forwarded,
/// and the UI hides the forward checkbox for it.
fn llm_forward_creds(cfg: &LlmConfig) -> Option<(String, String, String)> {
    let key = llm::load_api_key(cfg).ok().flatten()?;
    let provider = match cfg.provider {
        Provider::Nanogpt => "nanogpt",
        Provider::Zai => "zai",
        Provider::ZaiCoding => "zai-coding",
        _ => return None,
    };
    Some((provider.into(), key, cfg.model.clone()))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportArgs {
    blueprint: FamiliarBlueprint,
    user_name: Option<String>,
    pf_port: u16,
}

#[tauri::command]
async fn export_bootstrap(
    app: tauri::AppHandle,
    args: ExportArgs,
    #[allow(unused_variables)] llm: LlmConfig,
) -> Result<String, String> {
    let checklist = blueprint::run_checklist(&args.blueprint);
    if !checklist.passed {
        return Err(format!(
            "The blueprint does not pass Advoco's own quality rules yet: {}",
            checklist.failures.join("; ")
        ));
    }
    let settings = args.blueprint.render_settings(args.user_name);
    // Default save location: the user's Desktop — zero-navigation.
    let desktop = dirs_desktop(&app)?;
    let pkg = export::write_bootstrap_package(&desktop, &args.blueprint, &settings, args.pf_port)?;
    Ok(pkg.to_string_lossy().into_owned())
}

fn dirs_desktop(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // Prefer the real Desktop; fall back to home if redirected setups break it.
    if let Some(desktop) = app.path().home_dir().ok().map(|h| h.join("Desktop")) {
        if desktop.is_dir() {
            return Ok(desktop);
        }
    }
    app.path()
        .home_dir()
        .map_err(|_| "could not find your home folder".to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            test_llm,
            detect_pf,
            generate,
            apply_blueprint,
            export_bootstrap,
        ])
        .run(tauri::generate_context!())
        .expect("error while running advoco");
}
