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

use egaku::{KeyCombo, KeyMap};
use pauta_spec::{Catalog, Effect, Gate};

/// Build the keymap from the authored catalog.
#[must_use]
pub fn keymap(catalog: &Catalog) -> KeyMap<Effect> {
    let mut m = KeyMap::new();
    for a in catalog.actions() {
        m.bind(KeyCombo::key(&a.binding), a.effect);
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

    #[test]
    fn every_authored_action_reaches_the_keymap() {
        let c = catalog();
        let m = keymap(&c);
        assert_eq!(
            m.len(),
            c.actions().len(),
            "a chord was swallowed between the catalog and the keymap"
        );
        for a in c.actions() {
            assert_eq!(
                m.lookup(&KeyCombo::key(&a.binding)),
                Some(&a.effect),
                "chord `{}` did not resolve to its authored effect",
                a.binding
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
