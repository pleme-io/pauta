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
            validate_binding(&a.name, &a.binding)?;
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

/// Reject a `:binding` that cannot possibly reach the runtime.
///
/// `keys::keymap` builds each entry with `egaku::KeyCombo::key(&binding)`,
/// which stores the string as a **literal key name** and parses nothing. So a
/// binding that is not exactly what `egaku_term` delivers for some keypress
/// compiles, passes every duplicate check below, and then simply never fires.
/// The failure mode is silence — no error, no log, just a dead key — which is
/// the worst shape a config error can take.
///
/// Three ways to author one, all rejected here:
///
/// - **A modifier chord.** `"ctrl+s"` is one 6-character key name to
///   `KeyCombo::key`, and no keypress is ever named that. Modified chords
///   need `KeyCombo::new("s", vec!["ctrl".into()])`, which `:binding` has no
///   way to express — so authoring one is always a mistake, not a feature
///   this rejects.
/// - **An uppercase letter.** `egaku_term` lowercases every character key and
///   conveys the case as a `shift` modifier, so `"S"` is never delivered.
/// - **Whitespace.** A two-key sequence (`"C-x e"`) is a chord, not a key
///   name; `KeyMap` has no notion of a pending prefix.
///
/// This is a SHAPE check, deliberately not a vocabulary check. Enumerating
/// every name `egaku_term` emits would put a second copy of that list here,
/// in a crate that (by design — three dependencies) cannot see the first. The
/// exhaustive check lives where egaku IS visible: `keys.rs`'s
/// `every_authored_binding_is_reachable_by_a_real_keypress`, which drives real
/// `KeyEvent`s through `egaku_term::event::from_key_event` and asserts each
/// authored binding is something a keyboard can actually produce.
fn validate_binding(name: &str, binding: &str) -> Result<(), SpecError> {
    let bad = |why: &str| {
        let mut m = String::from("action `");
        m.push_str(name);
        m.push_str("` has an unreachable binding `");
        m.push_str(binding);
        m.push_str("`: ");
        m.push_str(why);
        Err(SpecError::Invalid(m))
    };

    if binding.is_empty() {
        return bad("a binding cannot be empty");
    }
    if binding.contains('+') {
        return bad(
            "modifier chords are not expressible — `KeyCombo::key` takes a literal key name, \
             so this would never match a keypress",
        );
    }
    if binding.chars().any(char::is_whitespace) {
        return bad("multi-key sequences are not supported — one key name per action");
    }
    if binding.chars().any(|c| c.is_ascii_uppercase()) {
        return bad(
            "key names are delivered lowercase (case arrives as a `shift` modifier), \
             so an uppercase name never matches",
        );
    }
    Ok(())
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

    #[test]
    /// The landmine this validation exists for. Before it, this catalog
    /// compiled, passed all three duplicate checks, and shipped a key that
    /// silently never fired — `KeyCombo::key("ctrl+s")` is a literal
    /// 6-character key name and no keypress is ever called that.
    #[test]
    fn a_modifier_chord_is_rejected_because_it_could_never_fire() {
        let src = r#"
(defpautaaction :name "stage" :binding "ctrl+s" :effect approve :gate binding)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("ctrl+s") && m.contains("modifier")),
            "a modifier chord is unreachable through KeyCombo::key; got {e:?}"
        );
    }

    #[test]
    fn an_uppercase_binding_is_rejected() {
        // egaku_term lowercases character keys and conveys case as a `shift`
        // modifier, so "S" is delivered as ("s", [shift]) and an authored "S"
        // never matches.
        let src = r#"
(defpautaaction :name "stage" :binding "S" :effect approve :gate binding)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("lowercase")),
            "an uppercase key name is never delivered; got {e:?}"
        );
    }

    #[test]
    fn a_multi_key_sequence_is_rejected() {
        let src = r#"
(defpautaaction :name "stage" :binding "C-x e" :effect approve :gate binding)
"#;
        let e = Catalog::from_source(src).unwrap_err();
        assert!(
            matches!(&e, SpecError::Invalid(m) if m.contains("multi-key")),
            "KeyMap has no pending-prefix state; got {e:?}"
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

    /// Non-vacuity: the rejections above must not be rejecting everything.
    /// The canonical catalog — and every ordinary key shape — still loads.
    #[test]
    fn ordinary_bindings_still_pass_validation() {
        assert!(Catalog::load().is_ok(), "the canonical catalog must load");
        for b in ["j", "enter", "esc", "tab", "/", "?", "f1", "1", "-"] {
            let mut src = String::from(r#"(defpautaaction :name "x" :binding ""#);
            src.push_str(b);
            src.push_str(r#"" :effect approve :gate binding)"#);
            assert!(
                Catalog::from_source(&src).is_ok(),
                "`{b}` is a real key name and must be accepted"
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
