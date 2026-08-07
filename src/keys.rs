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

    /// Every authored binding must be something a keyboard can actually
    /// produce.
    ///
    /// `pauta-spec` rejects the SHAPES that obviously cannot fire (a modifier
    /// chord, an uppercase name, a multi-key sequence), but it deliberately
    /// does not enumerate egaku's key-name vocabulary — that list lives in
    /// `egaku_term` and a second copy in a spec crate that cannot see the
    /// first is exactly the drift worth avoiding. This is the exhaustive half,
    /// placed here because this is where egaku IS visible.
    ///
    /// It does not mirror the vocabulary either. It drives real `KeyEvent`s
    /// through the REAL delivery function the runtime uses, collects what
    /// comes out, and asserts every authored binding is in that set. So the
    /// test stays correct when `egaku_term` adds or renames a key — there is
    /// nothing here to update.
    ///
    /// This is what catches the case shape-checking cannot: a plausible
    /// spelling that is simply the wrong word. `"escape"` and `"return"` are
    /// the fleet atlas's canonical names (`ishou_tokens::FleetKeybinds`), and
    /// egaku delivers `"esc"` and `"enter"` — so copying a chord straight off
    /// the atlas into this catalog produces a key that never fires.
    #[test]
    fn every_authored_binding_is_reachable_by_a_real_keypress() {
        use egaku_term::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        // Every bare keypress a terminal can deliver. No modifiers: a
        // `:binding` is a bare key name by construction.
        let mut codes: Vec<KeyCode> = Vec::new();
        for c in "abcdefghijklmnopqrstuvwxyz0123456789/?-=[];',.`\\".chars() {
            codes.push(KeyCode::Char(c));
        }
        codes.extend([
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
        ]);
        for n in 1..=12u8 {
            codes.push(KeyCode::F(n));
        }

        let reachable: Vec<KeyCombo> = codes
            .into_iter()
            .filter_map(|code| {
                egaku_term::event::from_key_event(&KeyEvent::new(code, KeyModifiers::NONE))
            })
            .collect();

        // Non-vacuity: if the delivery function stopped answering, every
        // assertion below would pass against an empty set.
        assert!(
            reachable.len() > 50,
            "only {} reachable combos — the probe itself is broken",
            reachable.len()
        );

        for a in catalog().actions() {
            let authored = KeyCombo::key(&a.binding);
            assert!(
                reachable.contains(&authored),
                "action `{}` binds `{}`, which no keypress delivers — it would never fire",
                a.name,
                a.binding
            );
        }
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
