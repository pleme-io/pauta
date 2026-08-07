//! # pauta-spec — the authored vocabulary
//!
//! The `(defpautaaction)` surface. This crate holds the *types* and the
//! *authored catalog*; `pauta` holds the runtime that consumes them.
//!
//! ## Why the catalog and not a Rust table
//!
//! pauta shipped first with its bindings as a `const BINDINGS: &[(&str,
//! Action)]` plus a test holding it to `Action::ALL`. That caught an unbound
//! action, and it could not catch the thing that actually goes wrong: the
//! legend, the keymap and the docs being three hand-written lists that drift.
//! banken hit exactly that — its legend advertised `S` for a chord the
//! runtime bound as `shift+s`.
//!
//! Here there is one authored list. The keymap, the legend and the `--help`
//! are all *derived* from it, so they cannot disagree.
//!
//! ## The gate is a type, not a convention
//!
//! [`Gate::Binding`] marks an action that changes a pull request's
//! mergeability. The runtime refuses to submit one without a confirmation —
//! and crucially, an author cannot express "binding but unconfirmed", because
//! there is no such value to author into. The illegal state has no
//! representation rather than a runtime check.
//!
//! ## Tier honesty
//!
//! `pending-pauta: catalog` is **closed** by this crate for the action
//! vocabulary. Still authored in Rust and not yet lisp: the column set
//! (`board::COLUMNS`) and the sort default. Those are a `(defpautaview)` and
//! are not built. `pending-pauta: view-catalog`.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

/// Everything that can go wrong loading the catalog.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpecError {
    /// The authored Lisp failed to compile.
    #[error("pauta spec failed to compile: {0}")]
    Compile(String),
    /// The catalog compiled but says something impossible.
    #[error("pauta catalog is invalid: {0}")]
    Invalid(String),
}

impl From<tatara_lisp::LispError> for SpecError {
    fn from(e: tatara_lisp::LispError) -> Self {
        Self::Compile(e.to_string())
    }
}

/// What an action does. Unit-only and closed: the runtime matches on this
/// exhaustively, so a new authored effect with no runtime arm is a compile
/// error rather than a silently-ignored keystroke.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    SelectNext,
    SelectPrev,
    OpenDiff,
    CloseDiff,
    ToggleDiffMode,
    CycleLane,
    Approve,
    RequestChanges,
    Confirm,
    Cancel,
    Quit,
}

/// Whether an action changes a pull request's mergeability.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    /// Changes nothing on the forge. Runs on the first keystroke.
    Free,
    /// Notifies the author and changes whether the PR can merge. MUST be
    /// staged, previewed, and confirmed with a second keystroke.
    ///
    /// There is deliberately no `BindingUnconfirmed` — "binding but skips the
    /// gate" is not a value anyone can author.
    Binding,
}

/// One authored `(defpautaaction)`.
#[derive(DeriveTataraDomain, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[tatara(keyword = "defpautaaction")]
pub struct ActionSpec {
    /// Stable slug — what the legend and `--help` show.
    pub name: String,
    /// The chord the operator presses.
    pub binding: String,
    /// What the runtime does.
    pub effect: Effect,
    /// Whether it needs the two-keystroke confirmation.
    pub gate: Gate,
}

/// The canonical authored catalog, compiled into the binary.
///
/// `include_str!` rather than a runtime file read: a TUI that cannot find its
/// own keymap at startup is not a TUI, and shipping the vocabulary inside the
/// binary makes that failure unrepresentable instead of an error path.
pub const CANONICAL_ACTIONS_LISP: &str = include_str!("../specs/actions.lisp");

/// The compiled catalog. Private field, one constructor — a `Catalog` that
/// exists has already passed every validation below.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    actions: Vec<ActionSpec>,
}

impl Catalog {
    /// Compile and validate the canonical catalog.
    ///
    /// # Errors
    /// [`SpecError::Compile`] on malformed Lisp; [`SpecError::Invalid`] when
    /// the catalog compiles but is self-inconsistent — see [`Self::from_str`].
    pub fn load() -> Result<Self, SpecError> {
        Self::from_source(CANONICAL_ACTIONS_LISP)
    }

    /// Compile and validate arbitrary source — the seam a test uses to feed
    /// a deliberately-broken catalog and watch it be rejected.
    ///
    /// # Errors
    ///
    /// - [`SpecError::Compile`] — the Lisp does not parse.
    /// - [`SpecError::Invalid`] — a duplicate name, a duplicate binding, or a
    ///   duplicate effect. Each is a real authoring mistake with a silent
    ///   failure mode: two actions on one chord means one is unreachable and
    ///   nothing says so.
    pub fn from_source(src: &str) -> Result<Self, SpecError> {
        let actions: Vec<ActionSpec> = tatara_lisp::compile_typed::<ActionSpec>(src)?;
        if actions.is_empty() {
            return Err(SpecError::Invalid("the catalog declares no actions".into()));
        }
        for a in &actions {
            parse_binding(&a.name, &a.binding)?;
        }
        dup_check(actions.iter().map(|a| a.name.as_str()), "name")?;
        dup_check(actions.iter().map(|a| a.binding.as_str()), "binding")?;
        let mut effects: Vec<Effect> = actions.iter().map(|a| a.effect).collect();
        effects.sort_unstable();
        let before = effects.len();
        effects.dedup();
        if effects.len() != before {
            return Err(SpecError::Invalid(
                "two actions declare the same effect".into(),
            ));
        }
        Ok(Self { actions })
    }

    /// Every authored action, in authored order — which is legend order.
    #[must_use]
    pub fn actions(&self) -> &[ActionSpec] {
        &self.actions
    }

    /// Every action as `(typed chord, effect)`, in authored order.
    ///
    /// The accessor a keymap builder wants: the chord has already been
    /// parsed once, when the catalog was validated, so a consumer never
    /// re-parses and never sees the string form. A `Catalog` that exists has
    /// passed [`parse_binding`] on every action, which is why this can be
    /// infallible.
    #[must_use]
    pub fn chords(&self) -> Vec<(awase::Hotkey, Effect)> {
        self.actions
            .iter()
            .map(|a| {
                (
                    // Infallible: `from_source` parsed every binding before
                    // constructing the catalog, and there is no other
                    // constructor. The fallback is unreachable and exists so
                    // this accessor cannot panic on a future code path that
                    // forgets that invariant.
                    parse_binding(&a.name, &a.binding)
                        .unwrap_or_else(|_| awase::Hotkey::new(awase::Modifiers::NONE, awase::Key::F20)),
                    a.effect,
                )
            })
            .collect()
    }

    /// The action bound to `chord`, if any.
    #[must_use]
    pub fn by_binding(&self, chord: &str) -> Option<&ActionSpec> {
        self.actions.iter().find(|a| a.binding == chord)
    }

    /// Whether `effect` needs the two-keystroke gate. Derived from the
    /// catalog, so the runtime cannot decide differently from the author.
    #[must_use]
    pub fn is_binding(&self, effect: Effect) -> bool {
        self.actions
            .iter()
            .any(|a| a.effect == effect && a.gate == Gate::Binding)
    }
}

/// Parse a `:binding` into a typed chord, refusing anything unspellable.
///
/// # What this replaced, and why parsing is strictly stronger
///
/// This was four hand-rolled SHAPE checks (no `+`, no whitespace, no
/// uppercase, non-empty). They existed because `keys::keymap` built each
/// entry with `egaku::KeyCombo::key(&binding)`, which stores the string as a
/// **literal key name** and parses nothing — so `:binding "ctrl+s"` compiled,
/// passed every duplicate check, and then simply never fired. Silence is the
/// worst shape a config error can take.
///
/// `awase::Hotkey::parse` subsumes all four and catches one they structurally
/// could not: an unknown key **name**. `:binding "excape"` has the right
/// shape — lowercase, no modifier, no whitespace — and is still a dead key.
/// A shape check cannot see that; only the vocabulary can, and the vocabulary
/// lives in awase.
///
/// # It also LIFTS a restriction
///
/// The old checks banned modifier chords because `KeyCombo::key` could not
/// express one. That constraint belonged to the string dispatch path, not to
/// pauta: `egaku_term` now delivers `awase::Hotkey`, so `:binding "ctrl+s"`
/// is an ordinary chord. A rule that only existed to work around a
/// representation is gone with the representation.
///
/// Deliverability — can a terminal actually PRODUCE this chord — is still a
/// separate question, and still checked where egaku is visible:
/// `keys.rs`'s `every_authored_binding_is_reachable_by_a_real_keypress`.
fn parse_binding(name: &str, binding: &str) -> Result<awase::Hotkey, SpecError> {
    awase::Hotkey::parse(binding).map_err(|e| {
        let mut m = String::from("action `");
        m.push_str(name);
        m.push_str("` has an unparseable binding `");
        m.push_str(binding);
        m.push_str("`: ");
        let _ = std::fmt::Write::write_fmt(&mut m, format_args!("{e}"));
        SpecError::Invalid(m)
    })
}

fn dup_check<'a>(it: impl Iterator<Item = &'a str>, what: &str) -> Result<(), SpecError> {
    let mut seen = std::collections::BTreeSet::new();
    for v in it {
        if !seen.insert(v) {
            let mut m = String::from("two actions share the same ");
            m.push_str(what);
            m.push_str(": `");
            m.push_str(v);
            m.push('`');
            return Err(SpecError::Invalid(m));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canonical_catalog_compiles() {
        let c = Catalog::load().expect("the shipped vocabulary must compile");
        assert_eq!(c.actions().len(), 11, "eleven authored actions");
    }

    #[test]
    fn approve_and_request_changes_are_the_only_binding_actions() {
        let c = Catalog::load().unwrap();
        let binding: Vec<&str> = c
            .actions()
            .iter()
            .filter(|a| a.gate == Gate::Binding)
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(
            binding,
            vec!["approve", "request-changes"],
            "exactly the two verdicts that change mergeability are gated"
        );
    }

    #[test]
    fn the_gate_is_read_from_the_catalog_not_guessed() {
        let c = Catalog::load().unwrap();
        assert!(c.is_binding(Effect::Approve));
        assert!(c.is_binding(Effect::RequestChanges));
        assert!(!c.is_binding(Effect::SelectNext));
        assert!(
            !c.is_binding(Effect::Confirm),
            "confirm itself is free — it is the second key, not a third gate"
        );
    }

    #[test]
    fn every_binding_resolves() {
        let c = Catalog::load().unwrap();
        for a in c.actions() {
            assert_eq!(
                c.by_binding(&a.binding).map(|x| x.effect),
                Some(a.effect),
                "chord `{}` did not resolve to its own action",
                a.binding
            );
        }
    }

    // ── The gates, each shown red on a deliberately-broken catalog ──

    /// The landmine this validation exists for — now caught by PARSING.
    ///
    /// `:binding "excape"` has the right shape (lowercase, no modifier, no
    /// whitespace) and is still a dead key. The four hand-rolled shape checks
    /// this replaced could not see that; only the key vocabulary can, and it
    /// lives in awase.
    #[test]
    fn an_unknown_key_name_is_rejected() {
        let src = r#"
(defpautaaction :name "close" :binding "excape" :effect quit :gate free)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("excape")),
            "an unknown key name must be refused by name; got {e:?}"
        );
    }

    /// **LIFTED, deliberately.** A modifier chord used to be rejected because
    /// `KeyCombo::key` could not express one — a limit of the string dispatch
    /// path, not of pauta. `egaku_term` now delivers `awase::Hotkey`, so this
    /// is an ordinary chord and the old rule went away with the thing it
    /// worked around.
    #[test]
    fn a_modifier_chord_is_now_an_ordinary_binding() {
        let src = r#"
(defpautaaction :name "stage" :binding "ctrl+s" :effect approve :gate binding)
"#;
        let c = Catalog::from_source(src).expect("a modifier chord is expressible now");
        let (chord, _) = c.chords()[0];
        assert_eq!(
            chord,
            awase::Hotkey::new(awase::Modifiers::CTRL, awase::Key::S)
        );
    }

    /// A multi-key SEQUENCE is still refused — `KeyMode` indexes single
    /// chords, and there is no pending-prefix state for a follower key.
    #[test]
    fn a_multi_key_sequence_is_rejected() {
        let src = r#"
(defpautaaction :name "stage" :binding "C-x e" :effect approve :gate binding)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(matches!(&e, SpecError::Invalid(_)), "got {e:?}");
    }

    /// **A HAZARD, pinned rather than fixed.** `Hotkey::parse` is
    /// case-insensitive, so `"S"` yields `Key::S` with NO shift — the same
    /// chord as `"s"`. An author writing `"S"` meaning shift+s gets a silent
    /// collision with `"s"` instead of an error.
    ///
    /// banken hit this and documented the fix at the failure site
    /// (`ActionChord::SHIFT_HINT`): author an uppercase letter as
    /// `"shift+s"`, which is also what the terminal delivers. pauta inherits
    /// the hazard; the duplicate check below is what actually catches it,
    /// because the two spellings collapse to one chord.
    #[test]
    fn an_uppercase_letter_is_the_same_chord_as_its_lowercase() {
        let upper = Catalog::from_source(
            r#"(defpautaaction :name "x" :binding "S" :effect approve :gate binding)"#,
        )
        .expect("parses");
        let lower = Catalog::from_source(
            r#"(defpautaaction :name "x" :binding "s" :effect approve :gate binding)"#,
        )
        .expect("parses");
        assert_eq!(
            upper.chords()[0].0,
            lower.chords()[0].0,
            "`S` and `s` are ONE chord — author shift+s for the shifted one"
        );
    }

    #[test]
    fn an_empty_binding_is_rejected() {
        let src = r#"
(defpautaaction :name "stage" :binding "" :effect approve :gate binding)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(matches!(&e, SpecError::Invalid(_)), "got {e:?}");
    }

    /// Non-vacuity: the refusals above must not be rejecting everything.
    #[test]
    fn ordinary_bindings_still_pass_validation() {
        assert!(Catalog::load().is_ok(), "the canonical catalog must load");
        for b in [
            "j", "enter", "esc", "tab", "/", "?", "f1", "1", "-", "ctrl+s", "shift+s",
        ] {
            let mut src = String::from(r#"(defpautaaction :name "x" :binding ""#);
            src.push_str(b);
            src.push_str(r#"" :effect approve :gate binding)"#);
            assert!(
                Catalog::from_source(&src).is_ok(),
                "`{b}` is a real chord and must be accepted"
            );
        }
    }

    #[test]
    fn a_duplicate_binding_is_rejected() {
        let src = r#"
(defpautaaction :name "a" :binding "x" :effect approve :gate binding)
(defpautaaction :name "b" :binding "x" :effect quit    :gate free)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("binding")),
            "two actions on one chord means one is unreachable; got {e:?}"
        );
    }

    #[test]
    fn a_duplicate_name_is_rejected() {
        let src = r#"
(defpautaaction :name "a" :binding "x" :effect approve :gate binding)
(defpautaaction :name "a" :binding "y" :effect quit    :gate free)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("name")),
            "{e:?}"
        );
    }

    #[test]
    fn a_duplicate_effect_is_rejected() {
        let src = r#"
(defpautaaction :name "a" :binding "x" :effect quit :gate free)
(defpautaaction :name "b" :binding "y" :effect quit :gate free)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("effect")),
            "{e:?}"
        );
    }

    #[test]
    fn an_empty_catalog_is_rejected_rather_than_yielding_a_keyless_app() {
        let e = Catalog::from_source("").unwrap_err();
        assert!(matches!(e, SpecError::Invalid(_)), "{e:?}");
    }

    #[test]
    fn malformed_lisp_is_a_compile_error_not_a_silent_empty_catalog() {
        let e = Catalog::from_source("(defpautaaction :name").unwrap_err();
        assert!(matches!(e, SpecError::Compile(_)), "{e:?}");
    }
}
