//! The keymap and the legend — both DERIVED from the authored catalog.
//!
//! There is no binding table in this file, and that is the point. It
//! previously held a `const BINDINGS: &[(&str, Action)]` plus a test holding
//! it to `Action::ALL` — which caught an unbound action and could not catch
//! the legend drifting from the runtime, because they were two hand-written
//! lists. banken hit exactly that: its legend advertised `S` for a chord the
//! runtime bound as `shift+s`.
//!
//! Now `pauta-spec`'s `(defpautaaction)` catalog is the single source, and
//! the keymap, the legend and the gate all read from it.

use pauta_spec::{Catalog, Effect, Gate};

/// Build the keymap from the authored catalog.
///
/// Typed end to end: the catalog hands over `awase::Hotkey` values it parsed
/// at validation time, and `egaku_term` delivers the same type, so there is
/// no projection between the authored chord and the delivered one — and
/// therefore no place for the two to disagree.
///
/// # Panics
///
/// If two actions bind the same chord. That is unreachable through
/// [`Catalog::from_source`], which rejects a duplicate `:binding` before a
/// `Catalog` can exist; this is a second, independent guard on the same
/// invariant rather than a real error path. Panicking beats `try_bind`'s
/// silent alternative — a displaced binding nothing reports.
#[must_use]
pub fn keymap(catalog: &Catalog) -> awase::KeyMode<Effect> {
    let mut m: awase::KeyMode<Effect> = awase::KeyMode::typed("default", false);
    for (chord, effect) in catalog.chords() {
        m.try_bind(awase::Binding::new(chord, effect))
            .unwrap_or_else(|displaced| {
                panic!(
                    "two actions bind `{chord}` ({:?} and {effect:?}) — the catalog's \
                     duplicate check should have refused this",
                    displaced.action
                )
            });
    }
    m
}

/// The legend, in authored order — most important first.
///
/// Derived, so it cannot advertise a chord the runtime does not bind. A gated
/// action is marked with `!`, because "this one will ask you to confirm" is
/// exactly what an operator wants to know *before* pressing it.
#[must_use]
pub fn legend_parts(catalog: &Catalog) -> Vec<String> {
    catalog
        .actions()
        .iter()
        .map(|a| {
            let mut s = String::from(&a.binding);
            s.push(':');
            s.push_str(&a.name);
            if a.gate == Gate::Binding {
                s.push('!');
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::load().expect("the shipped vocabulary must compile")
    }

    /// Every authored binding must be something a keyboard can actually
    /// produce.
    ///
    /// `pauta-spec` rejects what cannot be PARSED — it runs every `:binding`
    /// through `awase::Hotkey::parse`, so an unknown key name has no typed
    /// value. Parseable is not the same as pressable, though: `f20` and
    /// `volumeup` are both real awase keys that no terminal delivers. This is
    /// that second question.
    ///
    /// It used to be ~30 lines of hand-rolled probe here. `egaku_term` now
    /// ships it (`event::testing`), so this is one call — and the shared
    /// probe still mirrors no vocabulary: it drives synthetic events through
    /// the REAL delivery function, so there is nothing to update when a key
    /// is added. Its non-vacuity floor lives inside it.
    #[test]
    fn every_authored_binding_is_reachable_by_a_real_keypress() {
        let chords: Vec<awase::Hotkey> = catalog().chords().into_iter().map(|(c, _)| c).collect();
        egaku_term::event::testing::assert_all_deliverable(&chords);
    }

    #[test]
    fn every_authored_action_reaches_the_keymap() {
        let c = catalog();
        let m = keymap(&c);
        assert_eq!(
            m.len(),
            c.actions().len(),
            "a chord was swallowed between the catalog and the keymap"
        );
        for (chord, effect) in c.chords() {
            assert_eq!(
                m.find_binding(&chord, &awase::MatchContext::default())
                    .map(|b| b.action),
                Some(effect),
                "chord `{chord}` did not resolve to its authored effect"
            );
        }
    }

    /// The drift this file exists to prevent: the legend must describe
    /// exactly what the keymap binds, because both come from one list.
    #[test]
    fn the_legend_describes_exactly_what_is_bound() {
        let c = catalog();
        let legend = legend_parts(&c);
        assert_eq!(legend.len(), c.actions().len());
        for (part, action) in legend.iter().zip(c.actions()) {
            let mut want = String::from(&action.binding);
            want.push(':');
            assert!(
                part.starts_with(&want),
                "legend entry {part:?} does not name the chord it is bound to"
            );
        }
    }

    #[test]
    fn the_legend_marks_the_gated_actions() {
        let c = catalog();
        let legend = legend_parts(&c);
        let marked: Vec<&String> = legend.iter().filter(|p| p.ends_with('!')).collect();
        assert_eq!(
            marked.len(),
            2,
            "approve and request-changes must be visibly marked as gated: {legend:?}"
        );
    }

    #[test]
    fn the_gate_comes_from_the_catalog() {
        let c = catalog();
        assert!(c.is_binding(Effect::Approve));
        assert!(!c.is_binding(Effect::SelectNext));
    }
}
