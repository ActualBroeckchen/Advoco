# 🐈 Advoco

**Advoco** (*Latin: “I summon, I invoke”*) is the onboarding wizard for
[Proto-Familiar](https://github.com/ScarletPrinceEury/Proto-Familiar) — a small
desktop app that gives a brand-new Familiar a vivid, stable personality before
you ever say hello.

You describe your Familiar in one sentence (“a haughty cat”), say how they
should help you, and optionally share conversations you had with another AI
companion. Advoco sketches a full personality — voice, backstory, boundaries,
body language, examples of how they talk at their edges — you adjust anything
you like, and one click writes it all into Proto-Familiar. No terminals, ever.

## What it actually does

1. **You describe them.** One sentence is plenty. A name, chatlogs, and notes
   are optional.
2. **You say how they should help.** Gentle, firm, competitive, a mix, or
   “figure me out over time”. This shapes how they look after you.
3. **Advoco sketches.** An LLM you connect to (Nano-GPT and z.ai — the
   Proto-Familiar standards — plus Anthropic, OpenAI, OpenRouter, or any
   OpenAI-compatible server) fills a strict personality blueprint — it never
   writes code, only a structured description that Advoco validates against
   its own quality rules.
4. **You review.** Everything is editable in plain language.
5. **One click.** While Proto-Familiar is running, “Bring them to life”
   writes identity files, profiles, and the people/things from your logs into
   your Familiar — after taking a safety snapshot, so it's all reversible.

No Proto-Familiar running? “Save a bring-them-to-life folder” puts a
double-clickable package on your Desktop instead. Start Proto-Familiar,
double-click, done — no console window appears.

## Design principles

- **Zero terminals.** If a user has to open a console, we've lost them.
  Advoco is built for people dealing with executive dysfunction: every step
  is skippable, every default sensible, every button big.
- **The doctrine.** AI models drift toward an eager-to-please “assistant”
  register and autocomplete attentive companions into flirting. Advoco's
  generation prompts counteract this deliberately: relationships are named
  archetypes (never behavior bans), the animal body is the true and only
  form and shows in every action, warmth is defined *without* romance,
  firmness is part of the character's pride with explicit rules for when to
  be gentle instead, and the three most important lines — flirt deflection,
  excuse callout, gruff warmth — are written as example dialogue, because
  models imitate examples far harder than they follow rules.
- **Platonic animals & mythical beasts.** Familiars are animal or
  mythological companions, full stop.
- **Your keys stay yours.** API keys live in your OS keyring. Chatlogs go
  only to the LLM provider you chose and to your own local Proto-Familiar.
  Nothing phones home — Advoco has no telemetry at all.
- **Proto-Familiar stays in charge.** Advoco never touches databases
  directly; every write goes through Proto-Familiar's own API, so its
  snapshotting, consent gates, and memory pipeline do the real work.

## Known upstream issue (Proto-Familiar)

Proto-Familiar's `POST /api/entity/identity` with `mode: "update_section"`
silently fails: thalamus sends the MCP tool a parameter named `heading`,
but Phylactery's tool requires `section`, and the validation error comes
back as an `isError` response thalamus never checks — the HTTP API still
answers `{ok: true}` while nothing is written (verified against
0.10.98-alpha, 2026-08-15). Advoco therefore writes identity files with
`mode: "append"` (headings embedded in the content) and skips files that
already exist, so re-runs never duplicate. Once fixed upstream,
`update_section` becomes the better mode again.

## Building from source

Prerequisites: Rust (rustup), and a WebView2 runtime (preinstalled on
current Windows).

```
cd src-tauri
cargo run            # debug run
cargo build          # debug binary at target/debug/advoco.exe
```

The UI is plain HTML/CSS/JS in `ui/` with no build step — but note the
assets are embedded at compile time, so rebuild after editing them.

## License

GPL-3.0 — see [LICENSE](LICENSE).
