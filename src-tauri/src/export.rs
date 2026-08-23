//! The export fallback: a double-clickable, console-less bootstrap package
//! that works on Windows, macOS, and Linux.
//!
//! Output folder:
//!   Advoco-Bootstrap/
//!     Advoco-Bootstrap.vbs            ← Windows: double-click, hidden window
//!     Advoco-Bootstrap.app/          ← macOS: double-click, no Terminal
//!       Contents/{Info.plist, MacOS/Advoco-Bootstrap}
//!     Advoco-Bootstrap.desktop        ← Linux: double-click, Terminal=false
//!     apply.ps1                       ← Windows worker (native MessageBox)
//!     apply.sh                        ← macOS/Linux worker (native dialog)
//!     payload.json                    ← the rendered writes, for apply.ps1
//!     requests/                       ← the same writes, one pre-rendered HTTP
//!                                        body per file, for apply.sh (no JSON
//!                                        parser or `jq` needed on Unix)
//!
//! No single script language double-clicks cleanly on every OS, so each OS gets
//! its own console-less launcher over a shared, secret-free payload:
//!   * Windows — the .vbs runs apply.ps1 with a hidden window (the same trick
//!     Proto-Familiar's own launcher uses); results show in a MessageBox.
//!   * macOS — a shell-script .app bundle; LaunchServices runs it with no
//!     Terminal, and apply.sh reports via `osascript` dialogs. The bundle's
//!     executable bit is preserved through the shareable zip (see below).
//!   * Linux — a `Terminal=false` .desktop entry runs apply.sh, which reports
//!     via zenity/kdialog/notify-send (falling back to stdout).
//!
//! Every *script* stays pure ASCII (Windows PowerShell 5.1 reads BOM-less files
//! as ANSI); all non-ASCII personality text lives in the UTF-8 JSON data files.

use crate::blueprint::FamiliarBlueprint;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Files whose Unix executable bit must survive the shareable zip, matched
/// against the package-relative path. Without +x the macOS .app will not launch
/// and the Linux .desktop will not run.
fn needs_exec_bit(rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    rel == "apply.sh"
        || rel.ends_with(".desktop")
        || rel.ends_with("Advoco-Bootstrap.app/Contents/MacOS/Advoco-Bootstrap")
}

pub fn write_bootstrap_package(
    dir: &Path,
    bp: &FamiliarBlueprint,
    settings: &crate::blueprint::SettingsPatch,
    pf_port: u16,
) -> Result<PathBuf, String> {
    let pkg_dir = dir.join("Advoco-Bootstrap");
    std::fs::create_dir_all(&pkg_dir).map_err(|e| e.to_string())?;

    // Windows worker input: one JSON blob apply.ps1 parses with ConvertFrom-Json.
    let payload = build_payload(bp, settings, pf_port);
    std::fs::write(
        pkg_dir.join("payload.json"),
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Unix worker input: the same writes as pre-rendered HTTP bodies + a
    // tab-separated manifest, so apply.sh needs no JSON parser.
    write_requests_dir(&pkg_dir, bp, settings, pf_port)?;

    // Workers.
    std::fs::write(pkg_dir.join("apply.ps1"), apply_ps1()).map_err(|e| e.to_string())?;
    std::fs::write(pkg_dir.join("apply.sh"), apply_sh()).map_err(|e| e.to_string())?;

    // Per-OS console-less launchers.
    std::fs::write(pkg_dir.join("Advoco-Bootstrap.vbs"), bootstrap_vbs())
        .map_err(|e| e.to_string())?;
    std::fs::write(pkg_dir.join("Advoco-Bootstrap.desktop"), bootstrap_desktop())
        .map_err(|e| e.to_string())?;
    let macos = pkg_dir.join("Advoco-Bootstrap.app").join("Contents");
    std::fs::create_dir_all(macos.join("MacOS")).map_err(|e| e.to_string())?;
    std::fs::write(macos.join("Info.plist"), macos_info_plist()).map_err(|e| e.to_string())?;
    std::fs::write(macos.join("MacOS").join("Advoco-Bootstrap"), macos_applet())
        .map_err(|e| e.to_string())?;

    // Set exec bits locally too (matters when Advoco itself runs on Unix; a
    // no-op on Windows). The shareable zip sets them independently.
    set_local_exec_bits(&pkg_dir);

    Ok(pkg_dir)
}

/// Write the `requests/` directory: one pre-rendered HTTP request body per
/// file, plus a tab-separated manifest apply.sh walks. This is the Unix
/// worker's whole interface — it never has to assemble or parse JSON.
fn write_requests_dir(
    pkg_dir: &Path,
    bp: &FamiliarBlueprint,
    settings: &crate::blueprint::SettingsPatch,
    pf_port: u16,
) -> Result<(), String> {
    let req = pkg_dir.join("requests");
    std::fs::create_dir_all(&req).map_err(|e| e.to_string())?;

    let write = |name: &str, bytes: &[u8]| -> Result<(), String> {
        std::fs::write(req.join(name), bytes).map_err(|e| e.to_string())
    };
    let write_json = |name: &str, v: &serde_json::Value| -> Result<(), String> {
        write(name, serde_json::to_string(v).map_err(|e| e.to_string())?.as_bytes())
    };

    write("pf_port", pf_port.to_string().as_bytes())?;
    write("familiar_name", bp.name.as_bytes())?;
    write_json("snapshot.json", &serde_json::json!({}))?;

    let mut manifest = String::new();
    for (i, w) in bp.render_identity_writes().into_iter().enumerate() {
        let body = format!("identity-{i:02}.json");
        let append = format!("identity-{i:02}.append.json");
        write_json(
            &body,
            &serde_json::json!({
                "category": w.category,
                "filename": w.filename,
                "heading": w.heading,
                "content": w.content,
                "mode": "update_section",
            }),
        )?;
        // Older Proto-Familiar builds swallowed update_section; the fallback
        // re-sends the section as an append (apply.sh uses it only if a write
        // did not land — same verify-then-append logic as apply.ps1).
        write_json(
            &append,
            &serde_json::json!({
                "category": w.category,
                "filename": w.filename,
                "content": format!("## {}\n\n{}\n", w.heading, w.content),
                "mode": "append",
            }),
        )?;
        // category<TAB>filename<TAB>bodyfile<TAB>appendfile — filenames never
        // contain tabs or spaces, so plain `read`/`for` splitting is safe.
        manifest.push_str(&format!("{}\t{}\t{}\t{}\n", w.category, w.filename, body, append));
    }
    write("manifest.tsv", manifest.as_bytes())?;

    let patch = settings_patch_map(settings);
    if !patch.is_empty() {
        write_json(
            "settings.json",
            &serde_json::json!({ "settings": serde_json::Value::Object(patch) }),
        )?;
    }

    for (i, g) in bp.graph_entities.iter().enumerate() {
        write_json(
            &format!("graph-{i:02}.json"),
            &serde_json::json!({ "label": g.label, "type": g.node_type, "description": g.description }),
        )?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_local_exec_bits(pkg_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    for rel in [
        "apply.sh",
        "Advoco-Bootstrap.desktop",
        "Advoco-Bootstrap.app/Contents/MacOS/Advoco-Bootstrap",
    ] {
        let p = pkg_dir.join(rel);
        if let Ok(meta) = std::fs::metadata(&p) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&p, perm);
        }
    }
}

#[cfg(not(unix))]
fn set_local_exec_bits(_pkg_dir: &Path) {}

/// Zip a package folder for sharing (ready-made Familiars). Uses the `zip`
/// crate rather than PowerShell so it (a) runs on any host OS and (b) records
/// Unix permissions in the archive — without the 0o755 bit on apply.sh and the
/// .app executable, a macOS/Linux receiver's double-click would silently do
/// nothing. Returns the zip path.
pub fn zip_package(pkg_dir: &Path, familiar_name: &str) -> Result<PathBuf, String> {
    let safe: String = familiar_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let zip_path = pkg_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("Advoco-Familiar-{safe}.zip"));

    let top = pkg_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Advoco-Bootstrap".into());
    let file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    add_dir_to_zip(&mut zip, pkg_dir, pkg_dir, &top)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(zip_path)
}

/// Recursively add `dir` to the zip under `prefix`, preserving the executable
/// bit on the launchers. Entries are sorted for a deterministic archive.
fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    dir: &Path,
    prefix: &str,
) -> Result<(), String> {
    use zip::write::SimpleFileOptions;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy();
        let entry = format!("{prefix}/{name}");
        if path.is_dir() {
            let opts = SimpleFileOptions::default().unix_permissions(0o755);
            zip.add_directory(format!("{entry}/"), opts)
                .map_err(|e| e.to_string())?;
            add_dir_to_zip(zip, root, &path, &entry)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mode = if needs_exec_bit(&rel) { 0o755 } else { 0o644 };
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(mode);
            zip.start_file(entry, opts).map_err(|e| e.to_string())?;
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

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
    heading: &'a str,
    content: String,
}

/// The PF settings patch as a JSON object (camelCase keys), shared by the
/// Windows payload and the Unix `requests/settings.json`.
fn settings_patch_map(settings: &crate::blueprint::SettingsPatch) -> serde_json::Map<String, serde_json::Value> {
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
    patch
}

fn build_payload<'a>(bp: &'a FamiliarBlueprint, settings: &crate::blueprint::SettingsPatch, pf_port: u16) -> Payload<'a> {
    let patch = settings_patch_map(settings);
    Payload {
        familiar_name: &bp.name,
        pf_port,
        identity: bp
            .render_identity_writes()
            .into_iter()
            .map(|w| PayloadIdentity {
                category: w.category,
                filename: w.filename,
                heading: w.heading,
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
    // r###-delimited: the script contains `"#` and `"##` sequences (e.g.
    // "## heading) that would end shorter raw strings early.
    r###"# Advoco bootstrap applier - run by Advoco-Bootstrap.vbs (hidden window).
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

    # 2. Identity files. update_section is the intended mode (fixed upstream
    #    2026-08-16), but older Proto-Familiar builds swallowed it with a
    #    silent ok:true - so each write is VERIFIED by re-reading the store,
    #    with an append fallback for anything that did not land. Files that
    #    already exist are left untouched (never clobber a live Familiar).
    $existing = Invoke-RestMethod -Uri "$base/api/entity/identity" -Method Get
    $todo = @()
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
                heading  = $w.heading;  content  = $w.content
                mode     = 'update_section'
            }
            $todo += $w
        } catch { $failed += $w.filename }
    }
    if ($todo.Count -gt 0) {
        $after = Invoke-RestMethod -Uri "$base/api/entity/identity" -Method Get
        foreach ($w in $todo) {
            $landed = $false
            foreach ($cat in 'self','ward','relationship','custom') {
                foreach ($f in $after.$cat) {
                    if ($f.filename -eq $w.filename -and $cat -eq $w.category) { $landed = $true }
                }
            }
            if ($landed) { continue }
            try {
                $null = Post '/api/entity/identity' @{
                    category = $w.category; filename = $w.filename
                    content  = "## $($w.heading)`n`n$($w.content)`n"
                    mode     = 'append'
                }
            } catch { $failed += $w.filename }
        }
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
"###
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

fn apply_sh() -> &'static str {
    // Pure ASCII (see module note). POSIX sh + curl; no jq, no bash-isms. All
    // per-familiar data lives in requests/, so this script is static and
    // auditable. Console-less feedback via the first available native dialog.
    r#"#!/bin/sh
# Advoco bootstrap applier for macOS and Linux. Applies the pre-rendered
# requests/ bodies to a running Proto-Familiar on 127.0.0.1. Launched with no
# terminal by Advoco-Bootstrap.app (macOS) or Advoco-Bootstrap.desktop (Linux);
# you can also run it by hand:  sh apply.sh
set -u

here=$(cd "$(dirname "$0")" 2>/dev/null && pwd)
req="$here/requests"

# Show a message in whatever native dialog exists; fall back to stdout. Keep
# messages free of double quotes so no escaping is needed.
show() {
  if command -v osascript >/dev/null 2>&1; then
    osascript -e "display dialog \"$2\" with title \"$1\" buttons {\"OK\"} default button 1 with icon note" >/dev/null 2>&1
  elif command -v zenity >/dev/null 2>&1; then
    zenity --info --no-wrap --title="$1" --text="$2" >/dev/null 2>&1
  elif command -v kdialog >/dev/null 2>&1; then
    kdialog --title "$1" --msgbox "$2" >/dev/null 2>&1
  elif command -v notify-send >/dev/null 2>&1; then
    notify-send "$1" "$2" >/dev/null 2>&1
  else
    printf '%s: %s\n' "$1" "$2"
  fi
}

if ! command -v curl >/dev/null 2>&1; then
  show "Advoco bootstrap" "This needs curl, which was not found. Please install curl and try again."
  exit 1
fi
if [ ! -f "$req/pf_port" ]; then
  show "Advoco bootstrap" "This package looks incomplete (requests/pf_port is missing)."
  exit 1
fi

port=$(tr -dc '0-9' < "$req/pf_port")
base="http://127.0.0.1:$port"
name=$(tr -d '\n' < "$req/familiar_name" 2>/dev/null)
[ -n "$name" ] || name="Your familiar"

# Proto-Familiar must be running.
if ! curl -fsS -m 5 "$base/api/settings" >/dev/null 2>&1; then
  show "Advoco bootstrap" "Proto-Familiar is not running (port $port). Start Proto-Familiar first, then open Advoco-Bootstrap again."
  exit 1
fi

post() { curl -fsS -m 30 -X POST -H 'Content-Type: application/json' --data-binary @"$1" "$2" >/dev/null 2>&1; }
failed=""

# 1. Safety snapshot.
post "$req/snapshot.json" "$base/api/entity/snapshots"

# 2. Identity files. Skip any that already exist (never clobber a live
#    Familiar), then verify each write landed and append-fallback if not -
#    the same reliability dance apply.ps1 does on Windows.
existing=$(curl -fsS -m 15 "$base/api/entity/identity" 2>/dev/null)
todo=""
if [ -f "$req/manifest.tsv" ]; then
  tab=$(printf '\t')
  while IFS="$tab" read -r category filename bodyfile appendfile; do
    [ -n "${filename:-}" ] || continue
    if printf '%s' "$existing" | grep -qF "\"$filename\""; then continue; fi
    if post "$req/$bodyfile" "$base/api/entity/identity"; then
      todo="$todo $filename:$appendfile"
    else
      failed="$failed $filename"
    fi
  done < "$req/manifest.tsv"
fi
if [ -n "$todo" ]; then
  after=$(curl -fsS -m 15 "$base/api/entity/identity" 2>/dev/null)
  for item in $todo; do
    fn=${item%%:*}
    af=${item#*:}
    if printf '%s' "$after" | grep -qF "\"$fn\""; then continue; fi
    post "$req/$af" "$base/api/entity/identity" || failed="$failed $fn"
  done
fi

# 3. Settings patch (Proto-Familiar merges it).
if [ -f "$req/settings.json" ]; then
  curl -fsS -m 30 -X PUT -H 'Content-Type: application/json' --data-binary @"$req/settings.json" "$base/api/settings" >/dev/null 2>&1 || failed="$failed settings"
fi

# 4. Graph nodes (best effort).
for g in "$req"/graph-*.json; do
  [ -f "$g" ] || continue
  post "$g" "$base/api/entity/graph/nodes"
done

if [ -z "$failed" ]; then
  show "Advoco bootstrap" "$name is ready. Open Proto-Familiar and say hello."
else
  show "Advoco bootstrap" "Mostly done, but some parts failed:$failed. A safety snapshot was taken first, so nothing is lost."
fi
exit 0
"#
}

fn macos_applet() -> &'static str {
    // The .app bundle's executable: LaunchServices runs it with no Terminal.
    // It sits at Contents/MacOS/, three levels below the package root where
    // apply.sh and requests/ live.
    r#"#!/bin/sh
# macOS console-less launcher. Finder runs this via the .app bundle with no
# Terminal window; the real work is in apply.sh at the package root.
root=$(cd "$(dirname "$0")/../../.." 2>/dev/null && pwd)
exec /bin/sh "$root/apply.sh"
"#
}

fn macos_info_plist() -> &'static str {
    // LSUIElement hides the Dock icon for this one-shot agent app.
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Advoco Bootstrap</string>
	<key>CFBundleDisplayName</key>
	<string>Advoco Bootstrap</string>
	<key>CFBundleIdentifier</key>
	<string>com.advoco.bootstrap</string>
	<key>CFBundleVersion</key>
	<string>1.0</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleExecutable</key>
	<string>Advoco-Bootstrap</string>
	<key>LSUIElement</key>
	<true/>
</dict>
</plist>
"#
}

fn bootstrap_desktop() -> &'static str {
    // Linux console-less launcher. %k is the .desktop file's own path; the
    // inner sh cd's to its folder (stripping any file:// URI form) and runs
    // apply.sh without a terminal. If a file manager refuses to launch it
    // (many require "Allow launching" first), `sh apply.sh` is the fallback.
    r#"[Desktop Entry]
Type=Application
Version=1.0
Name=Advoco Bootstrap
Comment=Bring your Familiar to life in Proto-Familiar
Exec=sh -c 'f="$1"; case "$f" in file://*) f=${f#file://};; esac; cd "$(dirname "$f")" && exec sh ./apply.sh' advoco %k
Terminal=false
Icon=applications-other
Categories=Utility;
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::{FamiliarBlueprint, GraphEntity};

    fn sample_bp() -> FamiliarBlueprint {
        FamiliarBlueprint {
            name: "Marlowe".into(),
            species: "cat".into(),
            relationship_archetype: "an aloof guardian".into(),
            body_language: vec!["I flick my tail".into(), "I perch high".into(), "I headbutt".into()],
            warmth_expression: vec!["vigilance over sleep".into()],
            user_facts: vec!["struggles with mornings".into()],
            graph_entities: vec![GraphEntity {
                label: "Milo".into(),
                node_type: "pet".into(),
                relation: "owns".into(),
                description: "an elderly tabby".into(),
            }],
            reinforcement: "I am a cat - my only form.".into(),
            ..Default::default()
        }
    }

    fn scratch() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("advoco-export-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn build() -> (PathBuf, PathBuf) {
        let bp = sample_bp();
        let settings = bp.render_settings(Some("Ward".into()));
        let dir = scratch();
        let pkg = write_bootstrap_package(&dir, &bp, &settings, 7861).unwrap();
        (dir, pkg)
    }

    #[test]
    fn package_contains_every_per_os_launcher_and_data_file() {
        let (dir, pkg) = build();
        for rel in [
            "payload.json",
            "apply.ps1",
            "apply.sh",
            "Advoco-Bootstrap.vbs",
            "Advoco-Bootstrap.desktop",
            "Advoco-Bootstrap.app/Contents/Info.plist",
            "Advoco-Bootstrap.app/Contents/MacOS/Advoco-Bootstrap",
            "requests/pf_port",
            "requests/familiar_name",
            "requests/manifest.tsv",
            "requests/snapshot.json",
            "requests/identity-00.json",
            "requests/identity-00.append.json",
            "requests/settings.json",
            "requests/graph-00.json",
        ] {
            assert!(pkg.join(rel).is_file(), "missing package file: {rel}");
        }
        assert_eq!(
            std::fs::read_to_string(pkg.join("requests/pf_port")).unwrap(),
            "7861"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_generated_script_is_pure_ascii() {
        // Windows PowerShell 5.1 reads BOM-less files as ANSI, so a stray
        // non-ASCII byte in any launcher can corrupt it. Data files are exempt.
        let (dir, pkg) = build();
        for rel in [
            "apply.ps1",
            "apply.sh",
            "Advoco-Bootstrap.vbs",
            "Advoco-Bootstrap.desktop",
            "Advoco-Bootstrap.app/Contents/Info.plist",
            "Advoco-Bootstrap.app/Contents/MacOS/Advoco-Bootstrap",
        ] {
            let bytes = std::fs::read(pkg.join(rel)).unwrap();
            assert!(
                bytes.iter().all(|b| b.is_ascii()),
                "non-ASCII byte in generated script: {rel}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manifest_matches_identity_writes_and_bodies_are_valid_json() {
        let bp = sample_bp();
        let (dir, pkg) = build();
        let manifest = std::fs::read_to_string(pkg.join("requests/manifest.tsv")).unwrap();
        let lines: Vec<&str> = manifest.lines().collect();
        assert_eq!(lines.len(), bp.render_identity_writes().len());
        for line in lines {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 4, "manifest line must be 4 tab-separated cols");
            let (_cat, _fname, bodyfile, appendfile) = (cols[0], cols[1], cols[2], cols[3]);
            // Referenced bodies exist and are well-formed request JSON.
            let body: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(pkg.join("requests").join(bodyfile)).unwrap())
                    .unwrap();
            assert_eq!(body["mode"], "update_section");
            let append: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(pkg.join("requests").join(appendfile)).unwrap())
                    .unwrap();
            assert_eq!(append["mode"], "append");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zip_preserves_unix_exec_bits_on_launchers() {
        let bp = sample_bp();
        let (dir, pkg) = build();
        let zip_path = zip_package(&pkg, &bp.name).unwrap();

        let f = std::fs::File::open(&zip_path).unwrap();
        let mut ar = zip::ZipArchive::new(f).unwrap();
        let mut modes = std::collections::HashMap::new();
        for i in 0..ar.len() {
            let e = ar.by_index(i).unwrap();
            modes.insert(e.name().to_string(), e.unix_mode());
        }
        let exec = |name: &str| {
            let m = modes
                .get(name)
                .unwrap_or_else(|| panic!("zip entry missing: {name}"))
                .expect("entry has no unix mode");
            assert!(m & 0o111 != 0, "expected executable bit on {name} (mode {m:o})");
        };
        exec("Advoco-Bootstrap/apply.sh");
        exec("Advoco-Bootstrap/Advoco-Bootstrap.desktop");
        exec("Advoco-Bootstrap/Advoco-Bootstrap.app/Contents/MacOS/Advoco-Bootstrap");
        // A data file stays non-executable.
        let data = modes["Advoco-Bootstrap/payload.json"].expect("mode");
        assert!(data & 0o111 == 0, "payload.json should not be executable");

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&zip_path).ok();
    }
}
