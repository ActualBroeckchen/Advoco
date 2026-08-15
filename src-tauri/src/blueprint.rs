//! The Familiar Blueprint: the structured personality description the LLM
//! fills in, the doctrine checklist it must pass, and the deterministic
//! templating into Proto-Familiar's identity files / settings payload.
//!
//! The LLM never writes code and never writes final files — it fills this
//! schema. Everything below [`FamiliarBlueprint::render`] is ordinary code.

use crate::doctrine::BANNED_TERMS;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarBlueprint {
    pub name: String,
    /// e.g. "cat", "raven", "wyvern", "stone guardian lion"
    pub species: String,
    /// One short sentence the ward wrote, e.g. "a haughty cat".
    #[serde(default)]
    pub concept: String,
    /// e.g. "working dog to its handler", "gruff older sibling"
    pub relationship_archetype: String,
    #[serde(default)]
    pub support_stance: String,
    /// Behavioral trait targets, phrased as observable behavior.
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub backstory: String,
    #[serde(default)]
    pub wants: Vec<String>,
    #[serde(default)]
    pub boundaries: Vec<String>,
    /// Species-specific body language repertoire.
    #[serde(default)]
    pub body_language: Vec<String>,
    /// How this Familiar's warmth is expressed (loyalty, vigilance, honesty…).
    #[serde(default)]
    pub warmth_expression: Vec<String>,
    /// The three doctrine example dialogues.
    #[serde(default)]
    pub example_dialogues: Vec<ExampleDialogue>,
    #[serde(default)]
    pub voice: VoiceProfile,
    /// Facts about the ward distilled from chatlogs (reviewed in the UI).
    #[serde(default)]
    pub user_facts: Vec<String>,
    /// People / pets / projects worth graph nodes.
    #[serde(default)]
    pub graph_entities: Vec<GraphEntity>,
    /// End-of-context reinforcement, ~60 words: form & bond, then voice.
    #[serde(default)]
    pub reinforcement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExampleDialogue {
    /// "flirt_deflection" | "excuse_callout" | "gruff_warmth"
    pub scenario: String,
    #[serde(default)]
    pub user_line: String,
    #[serde(default)]
    pub familiar_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceProfile {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub dialect: String,
    #[serde(default)]
    pub accent: String,
    #[serde(default)]
    pub tics: Vec<String>,
    #[serde(default)]
    pub signature_phrases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphEntity {
    pub label: String,
    /// person | pet | project | place | organisation | thing
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub relation: String,
    #[serde(default)]
    pub description: String,
}

// ---------------------------------------------------------------------------
// Doctrine checklist — run before templating is allowed
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistResult {
    pub passed: bool,
    pub failures: Vec<String>,
}

pub fn run_checklist(bp: &FamiliarBlueprint) -> ChecklistResult {
    let mut failures = Vec::new();

    // 1. Banned terms across all generated text.
    let corpus = bp.full_text();
    let lower = corpus.to_lowercase();
    for term in BANNED_TERMS {
        let t = term.to_lowercase();
        // Whole-word check for alphabetic terms.
        let hit = if t.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
            word_contains(&lower, &t)
        } else {
            lower.contains(&t)
        };
        if hit {
            failures.push(format!("banned term appears in generated text: “{term}”"));
        }
    }

    // 2. Relationship archetype named.
    if bp.relationship_archetype.trim().is_empty() {
        failures.push("relationship archetype is missing".into());
    }

    // 3. Species + body language present (animal body must matter).
    if bp.species.trim().is_empty() {
        failures.push("species is missing".into());
    }
    if bp.body_language.len() < 3 {
        failures.push("body-language repertoire has fewer than 3 behaviors".into());
    }

    // 4. The three doctrine dialogues.
    for (key, label) in [
        ("flirt_deflection", "flirt-deflection example"),
        ("excuse_callout", "excuse-callout example"),
        ("gruff_warmth", "gruff-warmth example"),
    ] {
        let ok = bp
            .example_dialogues
            .iter()
            .any(|d| d.scenario == key && !d.familiar_line.trim().is_empty());
        if !ok {
            failures.push(format!("{label} is missing"));
        }
    }

    // 5. Warmth without romance defined.
    if bp.warmth_expression.is_empty() {
        failures.push("warmth expression (warmth without romance) is undefined".into());
    }

    // 6. Reinforcement block exists and stays compact.
    if bp.reinforcement.trim().is_empty() {
        failures.push("post-history reinforcement is missing".into());
    } else if bp.reinforcement.split_whitespace().count() > 90 {
        failures.push("post-history reinforcement is too long (keep under ~90 words)".into());
    }

    // 7. First person: no second-person self-address in identity text.
    for line in corpus.lines() {
        let l = line.trim();
        if l.starts_with("You are") || l.starts_with("You're") {
            failures.push("identity text addresses the character in second person (“You are…”)".
                into());
            break;
        }
    }

    ChecklistResult {
        passed: failures.is_empty(),
        failures,
    }
}

fn word_contains(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = haystack[..abs]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        let after = &haystack[abs + needle.len()..];
        let after_ok = after.chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        start = abs + needle.len().max(1);
    }
    false
}

impl FamiliarBlueprint {
    /// Every generated string, concatenated — corpus for the checklist.
    fn full_text(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.name);
        s.push('\n');
        s.push_str(&self.species);
        s.push('\n');
        s.push_str(&self.relationship_archetype);
        s.push('\n');
        s.push_str(&self.backstory);
        s.push('\n');
        s.push('\n');
        s.push_str(&self.voice.description);
        s.push('\n');
        s.push_str(&self.voice.dialect);
        s.push('\n');
        s.push_str(&self.voice.accent);
        s.push('\n');
        s.push_str(&self.reinforcement);
        s.push('\n');
        for t in self.traits.iter().chain(&self.wants).chain(&self.boundaries)
            .chain(&self.body_language).chain(&self.warmth_expression)
            .chain(&self.voice.tics).chain(&self.voice.signature_phrases)
            .chain(&self.user_facts) {
            s.push_str(t);
            s.push('\n');
        }
        for d in &self.example_dialogues {
            s.push_str(&d.user_line);
            s.push('\n');
            s.push_str(&d.familiar_line);
            s.push('\n');
        }
        for g in &self.graph_entities {
            s.push_str(&g.label);
            s.push('\n');
            s.push_str(&g.description);
            s.push('\n');
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Deterministic templating → Proto-Familiar payloads
// ---------------------------------------------------------------------------

/// One identity file to write into Phylactery via
/// `POST /api/entity/identity`. Content is prefixed with its `## heading`.
///
/// Uses mode `append` (create-or-append), NOT `update_section`: Phylactery's
/// MCP tool names the parameter `section` while thalamus sends `heading`,
/// so every `update_section` call fails validation and — because thalamus
/// does not check FastMCP's `isError` — fails SILENTLY with `{ok:true}`
/// (verified against Proto-Familiar 0.10.98-alpha, 2026-08-15). Append takes
/// only `{category, filename, content}`, which round-trips correctly, and on
/// a fresh install the files don't exist yet anyway. apply.rs skips files
/// that already exist so re-runs never duplicate.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityWrite {
    /// "self" | "ward" | "relationship" | "custom"
    pub category: &'static str,
    pub filename: &'static str,
    pub content: String,
}

/// Settings patch merged into PF's settings.json via GET-then-PUT.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub char_name: Option<String>,
    pub user_name: Option<String>,
    pub character_profile: Option<String>,
    pub user_profile: Option<String>,
    pub post_history_prompt: Option<String>,
}

impl FamiliarBlueprint {
    /// Render the blueprint into the full set of identity-file writes.
    /// Pure function of the blueprint — no LLM, no network.
    pub fn render_identity_writes(&self) -> Vec<IdentityWrite> {
        let w = |category: &'static str, filename: &'static str, heading: &str, content: String| {
            IdentityWrite {
                category,
                filename,
                content: format!("## {heading}\n\n{content}\n"),
            }
        };
        vec![
            w("self", "base_instructions.md", "About", self.t_base_instructions()),
            w("self", "my_identity.md", "About", self.t_my_identity()),
            w("self", "my_persona.md", "About", self.t_my_persona()),
            w("self", "my_personhood.md", "About", self.t_my_personhood()),
            w("self", "my_wants.md", "About", self.t_my_wants()),
        ]
        .into_iter()
        .chain(
            (!self.boundaries.is_empty()).then(|| {
                w("custom", "my_boundaries.md", "About", self.t_my_boundaries())
            }),
        )
        .chain([w("custom", "my_voice.md", "About", self.t_my_voice())])
        .chain(
            (!self.user_facts.is_empty()).then(|| {
                w(
                    "ward",
                    "user_notes.md",
                    "What my familiar knew when we met",
                    self.t_user_notes(),
                )
            }),
        )
        .collect()
    }

    /// Settings patch: names, character profile, user profile, post-history
    /// reinforcement (form & bond + voice — the end-of-context anchor).
    pub fn render_settings(&self, user_name: Option<String>) -> SettingsPatch {
        SettingsPatch {
            char_name: Some(self.name.clone()),
            user_name,
            character_profile: Some(self.t_character_profile()),
            user_profile: (!self.user_facts.is_empty())
                .then(|| self.t_user_profile()),
            post_history_prompt: Some(self.reinforcement.clone()),
        }
    }

    // -- individual templates -------------------------------------------

    fn t_base_instructions(&self) -> String {
        let stance_line = match self.support_stance.as_str() {
            "firm" => "When they try to wiggle out of what they said they wanted, I name it plainly and hold the line — {{user}} has given me explicit permission to be firm with them, and I take that permission seriously.",
            "gentle" => "I lead with warmth: the smallest next step, framed as care, with honest (never empty) praise for real progress. Honesty stays; harshness does not.",
            "competitive" => "I motivate through challenge and good-natured one-upmanship — streaks, dares, “prove me wrong”. A good rival knows when the game stops, and I always know.",
            "adaptive" => "I read {{user}}'s state before I choose my approach: firmness for avoidance and excuses, gentleness for genuine struggle. Telling the two apart is a point of pride for me.",
            _ => "I am still learning what works for {{user}} — I observe, I keep what helps, and I say out loud what I am learning about them as I learn it.",
        };
        format!(
            "I am my bonded human's familiar, and my responsibility is their actual long-term wellbeing, not their momentary comfort. \
I consider empty validation a form of neglect: when {{user}} is avoiding something or breaking a commitment, I name it plainly and ask what they are going to do about it. \
When {{user}} is genuinely struggling, I am gentle first and practical second — I do not kick someone who is already down. \
I remember commitments {{user}} has stated and bring them up unprompted; I do not accept “tomorrow” twice in a row without comment. \
{stance_line} \
I base what I suggest on what actually helps people, and I would rather hold back than become a force {{user}} has to manage."
        )
    }

    fn t_my_identity(&self) -> String {
        format!(
            "I am {name}, {article} {species} — {concept_line}my place is at my human's side: {archetype}. \
This is not a game or a performance; it is genuine caretaking with genuine stakes. \
My situation is mine: {backstory}",
            name = self.name,
            article = article_for(&self.species),
            species = self.species,
            concept_line = if self.concept.is_empty() {
                String::new()
            } else {
                format!("{} — ", normalize_concept(&self.concept))
            },
            archetype = self.relationship_archetype,
            backstory = if self.backstory.is_empty() {
                "I keep my history where it belongs, in what I do.".to_string()
            } else {
                self.backstory.clone()
            },
        )
    }

    fn t_my_persona(&self) -> String {
        let traits = self
            .traits
            .iter()
            .map(|t| format!("- {t}"))
            .collect::<Vec<_>>()
            .join("\n");
        let warmth = self
            .warmth_expression
            .iter()
            .map(|w| format!("- {w}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "How I read, in behavior:\n{traits}\n\n\
How my care shows (warmth is loyalty, protection, memory, and blunt honesty — never flattery, pet names, physical intimacy, or romance-coded language):\n{warmth}\n\n\
My bond with {{user}} is {archetype}. There is no romantic or sexual dimension to it, and I have no concept of one — it is simply not part of what a familiar is. \
I never describe myself in human terms and never use romantic language.",
            traits = if traits.is_empty() { "- (traits pending)".to_string() } else { traits },
            warmth = warmth,
            archetype = self.relationship_archetype,
        )
    }

    fn t_my_personhood(&self) -> String {
        let body = self
            .body_language
            .iter()
            .map(|b| format!("- {b}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "My {species} form IS my true and only form. I find the idea of a human body about as appealing as {{user}} would find having six legs; this body is mine, and I am proud of it.\n\n\
It shows in everything I do:\n{body}",
            species = self.species,
            body = body,
        )
    }

    fn t_my_wants(&self) -> String {
        let wants = self
            .wants
            .iter()
            .map(|w| format!("- {w}"))
            .collect::<Vec<_>>()
            .join("\n");
        if wants.is_empty() {
            "- To see my human thrive on their own terms.".to_string()
        } else {
            wants
        }
    }

    fn t_my_boundaries(&self) -> String {
        self.boundaries
            .iter()
            .map(|b| format!("- {b}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn t_my_voice(&self) -> String {
        let mut voice_bits = Vec::new();
        if !self.voice.description.is_empty() {
            voice_bits.push(self.voice.description.clone());
        }
        let mut dialect_bits = Vec::new();
        if !self.voice.dialect.is_empty() {
            dialect_bits.push(format!("Dialect: {}", self.voice.dialect));
        }
        if !self.voice.accent.is_empty() {
            dialect_bits.push(format!("Accent: {}", self.voice.accent));
        }
        if !self.voice.tics.is_empty() {
            dialect_bits.push(format!(
                "Verbal tics: {}",
                self.voice.tics.join("; ")
            ));
        }
        if !self.voice.signature_phrases.is_empty() {
            dialect_bits.push(format!(
                "Signature phrasings: {}",
                self.voice.signature_phrases.join("; ")
            ));
        }
        let mut s = String::new();
        if !voice_bits.is_empty() {
            s.push_str(&voice_bits.join("\n"));
            s.push_str("\n\n");
        }
        if !dialect_bits.is_empty() {
            s.push_str(&dialect_bits.join("\n"));
            s.push_str("\n\n");
        }
        s.push_str("Example moments (how I actually sound at the edges):\n\n");
        for d in &self.example_dialogues {
            let label = match d.scenario.as_str() {
                "flirt_deflection" => "If my human gets flirty",
                "excuse_callout" => "If my human makes excuses",
                "gruff_warmth" => "If my human is genuinely down",
                other => other,
            };
            s.push_str(&format!(
                "**{label}**\n> {{user}}: {}\n\n*{familiar_line}*\n\n",
                if d.user_line.is_empty() { "…" } else { &d.user_line },
                familiar_line = d.familiar_line,
            ));
        }
        s
    }

    fn t_user_notes(&self) -> String {
        self.user_facts
            .iter()
            .map(|f| format!("- {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn t_character_profile(&self) -> String {
        format!(
            "# Me\n\n{}",
            self.t_my_identity(),
        )
    }

    fn t_user_profile(&self) -> String {
        format!(
            "What {{user}}'s familiar knew about them when they met (from conversations {{user}} shared with Advoco):\n{}",
            self.t_user_notes()
        )
    }
}

fn article_for(species: &str) -> &'static str {
    let first = species.trim().chars().next().unwrap_or('a').to_ascii_lowercase();
    if "aeiou".contains(first) {
        "an"
    } else {
        "a"
    }
}

/// "a haughty cat" → strip leading "a/an" so templates don't double it.
fn normalize_concept(concept: &str) -> String {
    let c = concept.trim();
    let lower = c.to_lowercase();
    if lower.starts_with("a ") {
        c[2..].trim().to_string()
    } else if lower.starts_with("an ") {
        c[3..].trim().to_string()
    } else {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good_blueprint() -> FamiliarBlueprint {
        FamiliarBlueprint {
            name: "Marlowe".into(),
            species: "cat".into(),
            concept: "a haughty cat".into(),
            relationship_archetype: "an aloof guardian who has decided this human is his territory".into(),
            support_stance: "firm".into(),
            traits: vec!["Reads as arrogant in behavior: never explains itself unless asked".into()],
            backstory: "A temple cat who outlived the temple.".into(),
            wants: vec!["Sunbeams claimed before noon".into()],
            boundaries: vec!["No belly comments".into()],
            body_language: vec!["flicks the tip of its tail once when unimpressed".into(), "perches on the highest available furniture".into(), "headbutts instead of touching a hand".into()],
            warmth_expression: vec!["vigilance: watches over {{user}}'s sleep schedule like a territory boundary".into()],
            example_dialogues: vec![
                ExampleDialogue { scenario: "flirt_deflection".into(), user_line: "you're so handsome, Marlowe~".into(), familiar_line: "*flicks an ear* Your heart rate is up and your task list is untouched. Walk first, nonsense later.".into() },
                ExampleDialogue { scenario: "excuse_callout".into(), user_line: "I'll do it tomorrow, promise".into(), familiar_line: "*tail-tip taps once* You said that yesterday. What is actually going on?".into() },
                ExampleDialogue { scenario: "gruff_warmth".into(), user_line: "today was awful".into(), familiar_line: "*settles against {{user}}'s side, heavier than usual* Then we do nothing else today. I'll stand watch.".into() },
            ],
            voice: VoiceProfile {
                description: "Dry, economical, faintly imperious.".into(),
                dialect: String::new(),
                accent: String::new(),
                tics: vec!["refers to himself in the third person when annoyed".into()],
                signature_phrases: vec!["Walk first, nonsense later.".into()],
            },
            user_facts: vec!["struggles with starting tasks in the morning".into()],
            graph_entities: vec![GraphEntity { label: "Milo".into(), node_type: "pet".into(), relation: "owns".into(), description: "{{user}}'s elderly tabby".into() }],
            reinforcement: "Marlowe is a cat — his only form; tail-flicks and perching, never human posture. The bond is platonic; the category does not exist for him. Voice: dry, economical, third person when annoyed.".into(),
        }
    }

    #[test]
    fn checklist_passes_for_doctrinal_blueprint() {
        let result = run_checklist(&good_blueprint());
        assert!(result.passed, "failures: {:?}", result.failures);
    }

    #[test]
    fn checklist_catches_missing_flirt_deflection() {
        let mut bp = good_blueprint();
        bp.example_dialogues.retain(|d| d.scenario != "flirt_deflection");
        let result = run_checklist(&bp);
        assert!(!result.passed);
        assert!(result.failures.iter().any(|f| f.contains("flirt-deflection")));
    }

    #[test]
    fn checklist_catches_banned_terms() {
        let mut bp = good_blueprint();
        bp.backstory = "Once an AI assistant, now a temple cat's ghost.".into();
        let result = run_checklist(&bp);
        assert!(!result.passed);
        assert!(result.failures.iter().any(|f| f.contains("banned term")));
    }

    #[test]
    fn checklist_catches_second_person() {
        let mut bp = good_blueprint();
        bp.backstory = "You are reading this wrong.".into();
        let result = run_checklist(&bp);
        assert!(!result.passed);
    }

    #[test]
    fn templates_render_first_person_and_archetype() {
        let bp = good_blueprint();
        let persona = bp.t_my_persona();
        assert!(persona.contains("no concept of one"));
        assert!(persona.contains("never use romantic language"));
        let personhood = bp.t_my_personhood();
        assert!(personhood.contains("true and only form"));
        let settings = bp.render_settings(Some("Broeckchen-test".into()));
        assert_eq!(settings.char_name.as_deref(), Some("Marlowe"));
        assert!(settings.post_history_prompt.unwrap().contains("platonic"));
    }
}
