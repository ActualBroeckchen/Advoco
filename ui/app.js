/* Advoco wizard — plain JS, no build step.
   Every screen is a function that renders into #screen and returns nothing.
   State lives in `S`. Everything is skippable; defaults are sensible. */

const { invoke } = window.__TAURI__.core;

const S = {
  step: 0,
  concept: "",
  name: "",
  supportStance: "adaptive",
  extraNotes: "",
  chatlogs: [],      // [{ name, text }]
  llm: null,         // config from backend
  pfPort: 8742,
  blueprint: null,
  checklistPassed: false,
};

const STANCES = [
  { key: "gentle",     icon: "🕯️", title: "Gentle and encouraging",
    blurb: "Leads with warmth. Honest, but never harsh with me." },
  { key: "firm",       icon: "🪨", title: "Firm and strict",
    blurb: "Calls me out and doesn't let me wiggle out of things." },
  { key: "competitive",icon: "🏁", title: "Plays to my competitive side",
    blurb: "Streaks, dares, \"prove me wrong\". Knows when to stop, though." },
  { key: "adaptive",   icon: "🌊", title: "A mix — read the moment",
    blurb: "Firm when I'm avoiding things, gentle when I'm truly struggling." },
  { key: "discover",   icon: "🔍", title: "Let them figure me out",
    blurb: "They learn what works for me over time, and say what they learn." },
];

const el = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
};
const screen = () => document.getElementById("screen");
const setDots = (n, total) => {
  document.getElementById("step-dots").innerHTML = Array.from({ length: total },
    (_, i) => `<div class="dot${i === n ? " on" : ""}"></div>`).join("");
};

// ---------------------------------------------------------------------------
// Screen 1 — Who is joining you?
// ---------------------------------------------------------------------------

function screenWho() {
  S.step = 0; setDots(0, 5);
  screen().replaceChildren(el(`
    <div class="screen-inner fade-in">
      <h1>Who is joining you? 🐾</h1>
      <p class="sub">Describe your new Familiar in your own words — an animal or mythical
      beast, with as much attitude as you like. One sentence is plenty.</p>

      <textarea id="concept" rows="2" autofocus
        placeholder="e.g. a haughty cat who acts like he's doing me a favour"></textarea>

      <h2>Got a name in mind? <span class="hint" style="display:inline">(optional)</span></h2>
      <input type="text" id="fname" placeholder="Leave empty and they'll pick their own" />

      <h2>Conversations to bring along <span class="hint" style="display:inline">(optional)</span></h2>
      <p class="hint">Chats you had with another AI companion — exports from ChatGPT,
      SillyTavern and friends. Your new Familiar can start out knowing you.</p>
      <div class="drop" id="drop">
        <div id="droplabel">Drop chat files here, or click to choose</div>
        <input type="file" id="filepick" multiple
          accept=".json,.jsonl,.txt,.md,text/plain,application/json" />
      </div>

      <div class="row">
        <button class="primary big" id="next">Continue →</button>
      </div>
    </div>`));

  const concept = document.getElementById("concept");
  concept.value = S.concept;
  concept.addEventListener("input", () => { S.concept = concept.value; });
  const fname = document.getElementById("fname");
  fname.value = S.name;
  fname.addEventListener("input", () => { S.name = fname.value; });

  const drop = document.getElementById("drop");
  const label = document.getElementById("droplabel");
  const pick = document.getElementById("filepick");
  const showFiles = () => {
    if (!S.chatlogs.length) return;
    drop.classList.add("has");
    label.textContent = `${S.chatlogs.length} file(s): ` +
      S.chatlogs.map(f => f.name).join(", ");
  };
  drop.addEventListener("click", () => pick.click());
  drop.addEventListener("dragover", (e) => { e.preventDefault(); drop.classList.add("over"); });
  drop.addEventListener("dragleave", () => drop.classList.remove("over"));
  drop.addEventListener("drop", async (e) => {
    e.preventDefault(); drop.classList.remove("over");
    await readFiles(e.dataTransfer.files); showFiles();
  });
  pick.addEventListener("change", async () => { await readFiles(pick.files); showFiles(); });

  document.getElementById("next").addEventListener("click", () => {
    if (!S.concept.trim()) {
      concept.focus();
      concept.style.borderColor = "var(--bad)";
      setTimeout(() => concept.style.borderColor = "", 1200);
      return;
    }
    screenStance();
  });
  showFiles();
}

async function readFiles(fileList) {
  for (const f of fileList) {
    try {
      const text = await f.text();
      // Cap each file hard — the generation call has to stay digestible.
      const capped = text.length > 300_000 ? text.slice(0, 300_000) + "\n…[truncated]" : text;
      S.chatlogs.push({ name: f.name, text: capped });
    } catch { /* unreadable file: quietly skip */ }
  }
}

// ---------------------------------------------------------------------------
// Screen 2 — How should they help you?
// ---------------------------------------------------------------------------

function screenStance() {
  S.step = 1; setDots(1, 5);
  screen().replaceChildren(el(`
    <div class="screen-inner fade-in">
      <h1>How should they help you?</h1>
      <p class="sub">This shapes how your Familiar looks after you when things are hard.
      There's no wrong answer, and they'll adapt over time anyway.</p>
      <div class="cards" id="stancecards" style="grid-template-columns:1fr"></div>
      <div class="row">
        <button class="linky" id="back">← back</button>
        <button class="primary big" id="next">Continue →</button>
      </div>
    </div>`));

  const cards = document.getElementById("stancecards");
  for (const st of STANCES) {
    const c = el(`
      <div class="card${S.supportStance === st.key ? " sel" : ""}">
        <h3>${st.icon} ${st.title}</h3><p>${st.blurb}</p>
      </div>`);
    c.addEventListener("click", () => {
      S.supportStance = st.key;
      cards.querySelectorAll(".card").forEach(x => x.classList.remove("sel"));
      c.classList.add("sel");
    });
    cards.append(c);
  }
  document.getElementById("back").addEventListener("click", screenWho);
  document.getElementById("next").addEventListener("click", async () => {
    // Make sure a connection exists before generating.
    const cfg = await invoke("get_config");
    S.llm = cfg.llm; S.pfPort = cfg.pfPort;
    if (!cfg.hasApiKey) { screenConnection(screenExtras); return; }
    screenExtras();
  });
}

// ---------------------------------------------------------------------------
// Screen 3 — Anything to add? (all optional)
// ---------------------------------------------------------------------------

function screenExtras() {
  S.step = 2; setDots(2, 5);
  screen().replaceChildren(el(`
    <div class="screen-inner fade-in">
      <h1>Anything else they should know?</h1>
      <p class="sub">Totally optional. Things like quirks you love, how they should
      address you, boundaries, a bit of backstory you imagine for them…</p>
      <textarea id="notes" rows="5"
        placeholder="e.g. Speaks like a 1920s radio host. Hates being called cute. Calls me 'kid'."></textarea>
      <div class="row">
        <button class="linky" id="back">← back</button>
        <button class="primary big" id="go">Sketch them out ✨</button>
      </div>
    </div>`));
  const notes = document.getElementById("notes");
  notes.value = S.extraNotes;
  notes.addEventListener("input", () => { S.extraNotes = notes.value; });
  document.getElementById("back").addEventListener("click", screenStance);
  document.getElementById("go").addEventListener("click", generate);
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

async function generate() {
  S.step = 3; setDots(3, 5);
  screen().replaceChildren(el(`
    <div class="screen-inner center fade-in">
      <div class="spinner"></div>
      <h1>Sketching your Familiar…</h1>
      <p class="sub" id="genmsg">Giving them a shape, a voice, and opinions. This usually
      takes half a minute or so.</p>
    </div>`));

  const excerpts = S.chatlogs
    .map(f => `### ${f.name}\n${f.text}`)
    .join("\n\n")
    .slice(0, 280_000);

  const followups = S.extraNotes.trim() ? [S.extraNotes.trim()] : [];

  try {
    const out = await invoke("generate", {
      input: {
        concept: S.concept.trim(),
        name: S.name.trim(),
        supportStance: S.supportStance,
        chatlogExcerpts: excerpts,
        followupAnswers: followups,
      },
      llm: S.llm,
    });
    S.blueprint = out.blueprint;
    S.checklistPassed = out.checklistPassed;
    screenReview();
  } catch (e) {
    screenError(e, screenExtras);
  }
}

function screenError(e, retry) {
  screen().replaceChildren(el(`
    <div class="screen-inner center fade-in">
      <div class="bigface">🌫️</div>
      <h1>That didn't work</h1>
      <p class="sub err" style="max-width:560px;margin-inline:auto">${String(e)}</p>
      <div class="row" style="justify-content:center">
        <button class="linky" id="settings">⚙ Open connection settings</button>
        <button class="primary" id="retry">Try again</button>
      </div>
    </div>`));
  document.getElementById("retry").addEventListener("click", retry);
  document.getElementById("settings").addEventListener("click",
    () => screenConnection(retry));
}

// ---------------------------------------------------------------------------
// Screen 4 — Review & adjust
// ---------------------------------------------------------------------------

function screenReview() {
  S.step = 3; setDots(3, 5);
  const bp = S.blueprint;

  const dialogueLines = (bp.exampleDialogues || []).map(d => {
    const label = { flirt_deflection: "if you get flirty",
                    excuse_callout: "if you make excuses",
                    gruff_warmth: "if you're genuinely down" }[d.scenario] || d.scenario;
    return `<p><em>${label}:</em><br/>${escapeHtml(d.familiarLine)}</p>`;
  }).join("");

  screen().replaceChildren(el(`
    <div class="screen-inner fade-in">
      <h1>Meet ${escapeHtml(bp.name || "them")} ✨</h1>
      <p class="sub">This is the sketch. Change anything you like — every box here is
      editable, and it all goes into who they actually are.</p>

      ${S.checklistPassed ? "" : `
        <div class="review-card" style="border-color:var(--bad)">
          <h3>⚠ The sketch missed some of Advoco's quality rules</h3>
          <p class="hint">${(S.blueprint._failures || []).join("; ") ||
            "Some required pieces are missing — regenerating usually fixes this."}</p>
        </div>`}

      <div class="review-card">
        <div class="head"><h3>🐾 Who they are</h3><button class="faux linky" data-regen>↻ redo</button></div>
        <textarea data-field="identity">${escapeHtml(reviewIdentity(bp))}</textarea>
      </div>

      <div class="review-card">
        <div class="head"><h3>🗣️ How they sound</h3><button class="faux linky" data-regen>↻ redo</button></div>
        <textarea data-field="voice">${escapeHtml(reviewVoice(bp))}</textarea>
        <div class="lines">${dialogueLines}</div>
      </div>

      <div class="review-card">
        <div class="head"><h3>💗 How they'll treat you</h3></div>
        <textarea data-field="care">${escapeHtml(reviewCare(bp))}</textarea>
      </div>

      ${(bp.userFacts || []).length ? `
      <div class="review-card">
        <div class="head"><h3>🧠 What they'll already know about you</h3></div>
        <p class="hint">From the conversations you shared. Remove anything you'd rather
        they didn't start with.</p>
        <textarea data-field="userfacts">${escapeHtml((bp.userFacts || []).map(f => "- " + f).join("\n"))}</textarea>
      </div>` : ""}

      <div class="chips">
        <span class="chip">species: ${escapeHtml(bp.species || "?")}</span>
        <span class="chip">bond: ${escapeHtml(bp.relationshipArchetype || "?")}</span>
        <span class="chip">stance: ${escapeHtml(S.supportStance)}</span>
      </div>

      <div class="row">
        <button class="linky" id="regenall">↻ sketch a different take</button>
        <button class="primary big" id="next">Looks right →</button>
      </div>
    </div>`));

  // Edits write back into the blueprint's plain fields.
  bindField("identity", (v) => { bp.backstory = firstLine(v) || bp.backstory; bp.concept = restLines(v) || bp.concept; });
  bindField("voice", (v) => {
    bp.voice = bp.voice || {};
    const lines = v.split("\n").map(l => l.trim()).filter(Boolean);
    bp.voice.register = lines[0] || bp.voice.register || "";
    const quoted = lines.filter(l => l.startsWith('"') || l.startsWith('“'));
    if (quoted.length) bp.voice.styleReferences = quoted;
    const rest = lines.filter(l => !quoted.includes(l) && l !== lines[0]);
    if (rest.length) bp.voice.description = rest.join("\n");
  });
  bindField("care", (v) => { bp.warmthExpression = bulletList(v); });
  bindField("userfacts", (v) => { bp.userFacts = bulletList(v); });

  document.getElementById("regenall").addEventListener("click", generate);
  document.querySelectorAll("[data-regen]").forEach(b =>
    b.addEventListener("click", generate));
  document.getElementById("next").addEventListener("click", screenApply);
}

function reviewIdentity(bp) {
  return `${bp.backstory || ""}\n${bp.concept || ""}`.trim();
}
function reviewVoice(bp) {
  const v = bp.voice || {};
  return [v.register, v.description, v.dialect, v.accent,
          (v.tics || []).join("; "),
          (v.styleReferences || []).join("\n"),
          (v.signaturePhrases || []).join("; ")].filter(Boolean).join("\n");
}
function reviewCare(bp) {
  return (bp.warmthExpression || []).map(w => "- " + w).join("\n");
}
function bindField(field, apply) {
  const ta = document.querySelector(`[data-field="${field}"]`);
  if (!ta) return;
  ta.addEventListener("change", () => apply(ta.value));
}
const firstLine = (v) => v.split("\n")[0].trim();
const restLines = (v) => v.split("\n").slice(1).join("\n").trim();
const bulletList = (v) => v.split("\n").map(l => l.trim().replace(/^[-•]\s*/, ""))
  .filter(Boolean);

// ---------------------------------------------------------------------------
// Screen 5 — Bring them to life
// ---------------------------------------------------------------------------

function screenApply() {
  S.step = 4; setDots(4, 5);
  const bp = S.blueprint;
  // Only providers PF's own import pipeline can drive (its PROVIDER_URLS
  // map) can forward chatlogs — Nano-GPT and z.ai, the Familiar standards.
  const canForward = S.chatlogs.length > 0 && S.llm &&
    ["nanogpt", "zai", "zai-coding"].includes(S.llm.provider);

  screen().replaceChildren(el(`
    <div class="screen-inner center fade-in">
      <div class="bigface">🐈</div>
      <h1>Ready when you are</h1>
      <p class="sub">Proto-Familiar will take a safety snapshot first, then everything
      about ${escapeHtml(bp.name || "your Familiar")} is written in — reversible at any time.</p>

      <p id="pfstatus" class="hint">checking for Proto-Familiar…</p>

      ${canForward ? `
      <label style="display:block;margin:18px 0">
        <input type="checkbox" id="fwdlogs" checked />
        Also let ${escapeHtml(bp.name || "them")} remember the ${S.chatlogs.length}
        conversation(s) I shared
      </label>` : ""}

      <button class="primary big" id="apply">✨ Bring ${escapeHtml(bp.name || "them")} to life</button>

      <p class="hint" style="margin-top:26px">
        No Proto-Familiar right now?
        <button class="linky" id="export">Save a "bring them to life" folder</button> —
        double-click it any time after you start Proto-Familiar.
      </p>

      <p class="hint">
        Made a Familiar worth sharing?
        <button class="linky" id="exportzip">Save a shareable package (.zip)</button> —
        anyone can unzip it and double-click; they never need Advoco.
      </p>

      <div class="row" style="justify-content:center">
        <button class="linky" id="back">← back to the sketch</button>
      </div>
      <div class="steps-report" id="report" style="text-align:left"></div>
    </div>`));

  const status = document.getElementById("pfstatus");
  invoke("detect_pf", { pfPort: S.pfPort })
    .then(ok => {
      status.innerHTML = ok
        ? '<span class="ok">● Proto-Familiar is running.</span>'
        : '<span class="err">● Proto-Familiar isn\'t running — start it, then come back. (Or use the save-folder option below.)</span>';
      document.getElementById("apply").disabled = !ok;
    })
    .catch(() => { status.textContent = ""; });

  document.getElementById("back").addEventListener("click", screenReview);

  document.getElementById("apply").addEventListener("click", async () => {
    const btn = document.getElementById("apply");
    btn.disabled = true; btn.textContent = "bringing them to life…";
    const fwdBox = document.getElementById("fwdlogs");
    const chatlog = (fwdBox && fwdBox.checked) ? {
      content: S.chatlogs.map(f => f.text).join("\n\n"),
      selfName: S.name || null,
    } : null;
    try {
      const report = await invoke("apply_blueprint", {
        args: { blueprint: S.blueprint, userName: S.name || null,
                pfPort: S.pfPort, chatlog },
        llm: S.llm,
      });
      showReport(report, true);
    } catch (e) {
      showReport({ ok: false, steps: [{ step: "error", ok: false, detail: String(e) }] }, false);
    }
  });

  document.getElementById("export").addEventListener("click", async () => {
    try {
      const dir = await invoke("export_bootstrap", {
        args: { blueprint: S.blueprint, userName: S.name || null, pfPort: S.pfPort },
        llm: S.llm,
      });
      screen().querySelector(".screen-inner").prepend(el(`
        <div class="review-card" style="border-color:var(--good)">
          <h3>📁 Saved to your Desktop</h3>
          <p class="hint">${escapeHtml(dir)}<br/>
          Open Proto-Familiar, then double-click
          <b>Advoco-Bootstrap.vbs</b> inside that folder. That's all.</p>
        </div>`));
    } catch (e) { screenError(e, screenApply); }
  });

  document.getElementById("exportzip").addEventListener("click", async () => {
    try {
      const zip = await invoke("export_bootstrap", {
        args: { blueprint: S.blueprint, userName: null, pfPort: S.pfPort, asZip: true },
        llm: S.llm,
      });
      screen().querySelector(".screen-inner").prepend(el(`
        <div class="review-card" style="border-color:var(--good)">
          <h3>🎁 Shareable package saved</h3>
          <p class="hint">${escapeHtml(zip)}<br/>
          Send this zip to anyone: unzip, start Proto-Familiar,
          double-click <b>Advoco-Bootstrap.vbs</b>. They never need Advoco.
          (It carries no API keys and nothing about you.)</p>
        </div>`));
    } catch (e) { screenError(e, screenApply); }
  });
}

function showReport(report) {
  const box = document.getElementById("report");
  box.replaceChildren();
  for (const s of report.steps) {
    box.append(el(`
      <div class="step-line">
        <span class="mark">${s.ok ? '<span class="ok">✔</span>' : '<span class="err">✘</span>'}</span>
        <span><b>${escapeHtml(prettyStep(s.step))}</b><br/>
        <span class="detail">${escapeHtml(s.detail)}</span></span>
      </div>`));
  }
  if (report.ok) {
    box.prepend(el(`<h2 class="center">🎉 ${escapeHtml(S.blueprint.name || "Your Familiar")} is alive.</h2>
      <p class="sub center">Open Proto-Familiar and say hello — they're already themselves.</p>`));
  } else {
    box.prepend(el(`<h2 class="center">Partly done</h2>
      <p class="sub center">A safety snapshot was taken first, so nothing is lost.
      You can try the button again.</p>`));
  }
}

function prettyStep(step) {
  if (step === "safety-snapshot") return "Safety snapshot";
  if (step === "settings") return "Name & profiles";
  if (step === "chatlog-import") return "Shared memories";
  if (step.startsWith("identity/")) return "Who they are · " + step.split("/")[2];
  if (step.startsWith("graph/")) return "People & things · " + step.slice(6);
  return step;
}

// ---------------------------------------------------------------------------
// Connection settings (modal-style screen, returns to `after`)
// ---------------------------------------------------------------------------

function screenConnection(after) {
  const cfg = S.llm || { provider: "anthropic", baseUrl: "https://api.anthropic.com/v1",
                          model: "claude-sonnet-4-5", allowLocal: false };
  screen().replaceChildren(el(`
    <div class="screen-inner fade-in">
      <h1>⚙ One thing first: a brain</h1>
      <p class="sub">Advoco borrows an AI model to sketch your Familiar. Your key is
      stored safely in your Windows keyring and goes nowhere else.</p>

      <div class="cards">
        <div class="card${cfg.provider === "nanogpt" ? " sel" : ""}" data-p="nanogpt">
          <h3>Nano-GPT</h3><p>One key, many models. The Familiar standard.</p></div>
        <div class="card${cfg.provider === "zai" ? " sel" : ""}" data-p="zai">
          <h3>z.ai</h3><p>GLM models (glm-4.7, glm-5…). The Familiar standard.</p></div>
        <div class="card${cfg.provider === "zai-coding" ? " sel" : ""}" data-p="zai-coding">
          <h3>z.ai Coding Plan</h3><p>Use your Coding Plan quota (GLM models).</p></div>
        <div class="card${cfg.provider === "anthropic" ? " sel" : ""}" data-p="anthropic">
          <h3>Anthropic</h3><p>Claude — best personality sketches.</p></div>
        <div class="card${cfg.provider === "openai" ? " sel" : ""}" data-p="openai">
          <h3>OpenAI</h3><p>GPT models.</p></div>
        <div class="card${cfg.provider === "openrouter" ? " sel" : ""}" data-p="openrouter">
          <h3>OpenRouter</h3><p>One key, many providers.</p></div>
        <div class="card${cfg.provider === "custom" ? " sel" : ""}" data-p="custom">
          <h3>Other (OpenAI-compatible)</h3><p>Any server that speaks the same language</p></div>
      </div>

      <div id="customrow" style="${cfg.provider === "custom" ? "" : "display:none"}">
        <h2>Server address</h2>
        <input type="text" id="baseurl" value="${escapeAttr(cfg.baseUrl || "")}"
          placeholder="https://your-server.example.com/v1" />
        <label style="display:block;margin-top:10px" class="hint">
          <input type="checkbox" id="allowlocal" ${cfg.allowLocal ? "checked" : ""} />
          Allow local model endpoints (Ollama, LM Studio…) — normally blocked for safety
        </label>
      </div>

      <h2>API key</h2>
      <input type="password" id="apikey" placeholder="paste your key here" />

      <h2>Model <span class="hint" style="display:inline">(a good default is pre-filled)</span></h2>
      <input type="text" id="model" value="${escapeAttr(cfg.model || "")}" />

      <p id="testout" class="hint"></p>

      <div class="row">
        <button class="linky" id="cancel">cancel</button>
        <button class="primary" id="save" disabled>Save & continue →</button>
      </div>
    </div>`));

  const defaults = {
    nanogpt: ["https://nano-gpt.com/api/v1", "gpt-4o-mini"],
    zai: ["https://api.z.ai/api/paas/v4", "glm-4.7"],
    "zai-coding": ["https://api.z.ai/api/coding/paas/v4", "glm-4.7"],
    anthropic: ["https://api.anthropic.com/v1", "claude-sonnet-4-5"],
    openai: ["https://api.openai.com/v1", "gpt-4.1"],
    openrouter: ["https://openrouter.ai/api/v1", "anthropic/claude-sonnet-4.5"],
    custom: ["", ""],
  };
  let provider = cfg.provider;
  const model = document.getElementById("model");
  const customRow = document.getElementById("customrow");
  const apikey = document.getElementById("apikey");
  const testout = document.getElementById("testout");
  const save = document.getElementById("save");

  screen().querySelectorAll("[data-p]").forEach(c =>
    c.addEventListener("click", () => {
      provider = c.dataset.p;
      screen().querySelectorAll("[data-p]").forEach(x => x.classList.remove("sel"));
      c.classList.add("sel");
      customRow.style.display = provider === "custom" ? "" : "none";
      const [b, m] = defaults[provider];
      model.value = m;
      if (provider !== "custom") document.getElementById("baseurl").value = b;
    }));

  apikey.addEventListener("input", () => { save.disabled = !apikey.value.trim(); });

  save.addEventListener("click", async () => {
    save.disabled = true; save.textContent = "Saving…";
    const llmCfg = {
      provider,
      baseUrl: provider === "custom" ? document.getElementById("baseurl").value.trim()
                                      : defaults[provider][0],
      model: model.value.trim() || defaults[provider][1],
      allowLocal: document.getElementById("allowlocal").checked,
    };
    try {
      await invoke("set_config", {
        llm: llmCfg, pfPort: S.pfPort,
        apiKey: apikey.value.trim() || null,
      });
      testout.innerHTML = '<span class="ok">checking the connection…</span>';
      await invoke("test_llm", { llm: llmCfg });
      S.llm = llmCfg;
      after();
    } catch (e) {
      save.disabled = false; save.textContent = "Save & continue →";
      testout.innerHTML = `<span class="err">${escapeHtml(String(e))}</span>`;
    }
  });
  document.getElementById("cancel").addEventListener("click", after);
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

function escapeHtml(s) {
  return String(s ?? "").replace(/[&<>"']/g, c =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}
function escapeAttr(s) { return escapeHtml(s); }

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

document.getElementById("btn-settings").addEventListener("click", async () => {
  if (!S.llm) {
    const cfg = await invoke("get_config");
    S.llm = cfg.llm; S.pfPort = cfg.pfPort;
  }
  screenConnection(() => screenWho());
});

invoke("get_config").then(cfg => { S.llm = cfg.llm; S.pfPort = cfg.pfPort; });
screenWho();
