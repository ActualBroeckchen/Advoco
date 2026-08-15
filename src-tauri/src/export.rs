//! The export fallback: a double-clickable, console-less bootstrap package.
//!
//! Output folder:
//!   Advoco-Bootstrap/
//!     Advoco-Bootstrap.vbs   ← the thing the user double-clicks (hidden window)
//!     apply.ps1              ← does the work: snapshot → identity → settings → graph
//!     payload.json           ← the rendered writes (no secrets, no LLM needed)
//!
//! The .vbs pattern is the same one Proto-Familiar's own launcher uses:
//! WScript.Shell.Run with a hidden window flag, so no console ever appears,
//! and the PowerShell script shows a native MessageBox when it finishes.

use crate::blueprint::FamiliarBlueprint;
use std::path::Path;

pub fn write_bootstrap_package(
    dir: &Path,
    bp: &FamiliarBlueprint,
    settings: &crate::blueprint::SettingsPatch,
    pf_port: u16,
) -> Result<PathBuf, String> {
    let pkg_dir = dir.join("Advoco-Bootstrap");
    std::fs::create_dir_all(&pkg_dir).map_err(|e| e.to_string())?;

    let payload = build_payload(bp, settings, pf_port);
    std::fs::write(
        pkg_dir.join("payload.json"),
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    std::fs::write(pkg_dir.join("apply.ps1"), apply_ps1()).map_err(|e| e.to_string())?;
    std::fs::write(pkg_dir.join("Advoco-Bootstrap.vbs"), bootstrap_vbs()).map_err(|e| e.to_string())?;

    Ok(pkg_dir)
}

use std::path::PathBuf;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload<'a> {
    familiar_name: &'a str,
    pf_port: u16,
    identity: Vec<PayloadIdentity<'a>>,
    settings: serde_json::Value,
    graph: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PayloadIdentity<'a> {
    category: &'a str,
    filename: &'a str,
    content: String,
}

fn build_payload<'a>(bp: &'a FamiliarBlueprint, settings: &crate::blueprint::SettingsPatch, pf_port: u16) -> Payload<'a> {
    let mut patch = serde_json::Map::new();
    if let Some(v) = &settings.char_name {
        patch.insert("charName".into(), serde_json::json!(v));
    }
    if let Some(v) = &settings.user_name {
        patch.insert("userName".into(), serde_json::json!(v));
    }
    if let Some(v) = &settings.character_profile {
        patch.insert("characterProfile".into(), serde_json::json!(v));
    }
    if let Some(v) = &settings.user_profile {
        patch.insert("userProfile".into(), serde_json::json!(v));
    }
    if let Some(v) = &settings.post_history_prompt {
        patch.insert("postHistoryPrompt".into(), serde_json::json!(v));
    }
    Payload {
        familiar_name: &bp.name,
        pf_port,
        identity: bp
            .render_identity_writes()
            .into_iter()
            .map(|w| PayloadIdentity {
                category: w.category,
                filename: w.filename,
                content: w.content,
            })
            .collect(),
        settings: serde_json::Value::Object(patch),
        graph: bp
            .graph_entities
            .iter()
            .map(|g| serde_json::json!({ "label": g.label, "type": g.node_type, "description": g.description }))
            .collect(),
    }
}

fn apply_ps1() -> &'static str {
    r#"# Advoco bootstrap applier - run by Advoco-Bootstrap.vbs (hidden window).
# Applies payload.json to a running Proto-Familiar instance. No console output
# by design; the result is shown as a native MessageBox popup.
# (-Silent is a testing affordance: popups become console output.)
param([switch]$Silent)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework

$here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$p      = Get-Content (Join-Path $here 'payload.json') -Raw | ConvertFrom-Json
$base   = "http://127.0.0.1:$($p.pfPort)"
$failed = @()

function Post($path, $body) {
    return Invoke-RestMethod -Uri "$base$path" -Method Post -ContentType 'application/json' -Body ($body | ConvertTo-Json -Depth 8)
}
function Show($title, $text) {
    if ($Silent) { Write-Output "$title - $text"; return }
    [System.Windows.MessageBox]::Show($text, $title, 'OK', 'Information') | Out-Null
}

try {
    # Proto-Familiar must be running.
    $null = Invoke-RestMethod -Uri "$base/api/settings" -Method Get -TimeoutSec 5
} catch {
    Show 'Advoco bootstrap', "Proto-Familiar is not running (port $($p.pfPort)).`nStart Proto-Familiar first, then double-click Advoco-Bootstrap again."
    exit 1
}

try {
    # 1. Safety snapshot.
    $null = Post '/api/entity/snapshots' @{}

    # 2. Identity files - append mode, skipping ones that already exist
    #    (update_section is broken upstream: param name mismatch between
    #    thalamus and Phylactery fails silently with ok:true).
    $existing = Invoke-RestMethod -Uri "$base/api/entity/identity" -Method Get
    foreach ($w in $p.identity) {
        $already = $false
        foreach ($cat in 'self','ward','relationship','custom') {
            foreach ($f in $existing.$cat) {
                if ($f.filename -eq $w.filename -and $cat -eq $w.category) { $already = $true }
            }
        }
        if ($already) { continue }
        try {
            $null = Post '/api/entity/identity' @{
                category = $w.category; filename = $w.filename
                content  = $w.content;  mode     = 'append'
            }
        } catch { $failed += $w.filename }
    }

    # 3. Settings patch (PF merges it with existing settings).
    if ($p.settings.PSObject.Properties.Count -gt 0) {
        try {
            $null = Invoke-RestMethod -Uri "$base/api/settings" -Method Put -ContentType 'application/json' -Body (@{ settings = $p.settings } | ConvertTo-Json -Depth 8)
        } catch { $failed += 'settings' }
    }

    # 4. Graph nodes (best effort).
    foreach ($g in $p.graph) {
        try { $null = Post '/api/entity/graph/nodes' $g } catch { }
    }

    if ($failed.Count -eq 0) {
        Show 'Advoco bootstrap', "$($p.familiarName) is ready.`nOpen Proto-Familiar and say hello."
    } else {
        Show 'Advoco bootstrap', "Mostly done, but some parts failed:`n$($failed -join ', ')`n`nA safety snapshot was taken first - nothing is lost."
    }
    exit 0
} catch {
    Show 'Advoco bootstrap', "Something went wrong before anything was written:`n$($_.Exception.Message)"
    exit 1
}
"#
}

fn bootstrap_vbs() -> &'static str {
    // NOTE: both scripts stay pure ASCII on purpose - Windows PowerShell 5.1
    // reads BOM-less files as ANSI, and UTF-8 punctuation (em dash, curly
    // quotes) can decode into bytes that terminate PowerShell strings early.
    r#"' Advoco bootstrap launcher - runs apply.ps1 with NO console window,
' exactly like Proto-Familiar's own launcher. Double-click me.
Set sh = CreateObject("WScript.Shell")
psDir = Left(WScript.ScriptFullName, InStrRev(WScript.ScriptFullName, "\"))
cmd = "powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & psDir & "apply.ps1"""
sh.Run cmd, 0, False
"#
}
