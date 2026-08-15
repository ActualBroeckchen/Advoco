# 🐈 Advoco

**Advoco** (*Latin: "I summon, I invoke"*) is the onboarding wizard for
[Proto-Familiar](https://github.com/ScarletPrinceEury/Proto-Familiar) — a small
desktop app that gives a brand-new Familiar a vivid, stable personality before
you ever say hello.

## Using Advoco

You don't build anything. Grab the Windows installer
(`Advoco_x64-setup.exe`) from the repository's Releases page, run it, and
start Advoco from your Start menu. That's it.

1. **Who is joining you?** Describe your Familiar in your own words — one
   sentence is plenty ("a haughty cat"). A name is optional; so are
   conversations from another AI companion you'd like them to know you from.
2. **How should they help you?** Gentle, firm, competitive, a mix, or "figure
   me out over time". This shapes how they look after you — it's not a
   nicety, it steers their caretaking instincts from day one.
3. **Connect a brain.** Advoco borrows an AI model to sketch the personality:
   Nano-GPT and z.ai (the Proto-Familiar standards), plus Anthropic, OpenAI,
   OpenRouter, or any compatible server. Your key stays in your Windows
   keyring.
4. **Meet the sketch.** Everything is shown in plain language and editable —
   who they are, how they sound (with sample lines in *their* voice), how
   they'll treat you, what they already know about you.
5. **Bring them to life.** With Proto-Familiar running, one click writes it
   all in — after taking a safety snapshot, so it's fully reversible. Then
   open Proto-Familiar and say hello; they're already themselves.

No Proto-Familiar running yet? **Save a "bring them to life" folder** to your
Desktop instead, and double-click it whenever you're ready. No console window
ever appears.

## Sharing ready-made Familiars

On the last screen, **Save a shareable package (.zip)** turns your sketch into
something anyone can use: they unzip, start Proto-Familiar, and double-click
`Advoco-Bootstrap.vbs`. The receiver never needs Advoco, an LLM key, or a
terminal — the package carries everything and writes into their Phylactery
through Proto-Familiar's own API.

Packages are private by design: anything distilled from *your* conversations
(facts about you, the people and pets in your chats) is stripped before the
zip is built. What ships is the Familiar — nothing about you. No API keys are
ever included.

## What Advoco actually does

An LLM never writes code here. It fills a strict **Familiar Blueprint** — a
structured personality description — which Advoco validates against its own
quality rules and then renders, with ordinary code, into exactly the formats
Proto-Familiar understands: identity files, the four core prompts in settings,
an end-of-context reminder (form, bond, and voice), and optional knowledge-
graph nodes. Imported conversations are handed to Proto-Familiar's *own*
consent-gated memory pipeline, so your Familiar remembers them under your
privacy settings.

## Design principles

- **Zero terminals.** If a user has to open a console, we've lost them.
  Every step is skippable, every default sensible, every button big.
- **The doctrine.** AI models drift toward an eager-to-please "assistant"
  register and autocomplete attentive companions into flirting. Advoco's
  generation rules counteract it: relationships are *named archetypes*
  (never behavior bans), the animal body is the true and only form and shows
  in every action, warmth is defined without romance, firmness is part of the
  character's pride with explicit rules for when to be gentle instead, and
  the three most important lines — flirt deflection, excuse callout, gruff
  warmth — are written as *example dialogue in that specific Familiar's
  voice*, because models imitate examples far harder than they follow rules.
- **A voice, not a setting.** Every Familiar gets a character-card style
  speech profile: a dense register line and quoted style-reference lines
  that could only belong to them — plus texture anchors from Myers-Briggs,
  Enneagram, and TVTropes (the *Bond Animal* trope carries exactly the
  nuances a Familiar needs).
- **Platonic animals & mythical beasts.** Familiars are animal or
  mythological companions, full stop.
- **Your keys and chats stay yours.** Keys in the OS keyring; chats go only
  to your chosen provider and your own Proto-Familiar; no telemetry, ever.
- **Proto-Familiar stays in charge.** Advoco never touches databases
  directly; every write goes through Proto-Familiar's API, so its
  snapshotting, consent gates, and memory pipeline do the real work.

## Reliability notes

Advoco never trusts a success response alone: every identity write is
verified by re-reading the store, and anything that didn't land falls back
transparently to a compatibility path — so a Familiar arrives complete even
on older Proto-Familiar builds. Re-runs skip files that already exist, so an
existing Familiar's identity is never clobbered.

## For developers: building from source

Prerequisites: Rust (rustup) and a WebView2 runtime (preinstalled on current
Windows). This is only for developing Advoco — users never need this section.

```
cd src-tauri
cargo run            # debug run
cargo build          # debug binary at target/debug/advoco.exe
```

The UI is plain HTML/CSS/JS in `ui/` with no build step — but note the
assets are embedded at compile time, so rebuild (touch `build.rs`) after
editing them. The generated `.ps1`/`.vbs` scripts must stay pure ASCII:
PowerShell 5.1 reads BOM-less files as ANSI, and UTF-8 punctuation can
decode into string terminators.

## License

GPL-3.0 — see [LICENSE](LICENSE).
