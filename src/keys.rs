//! The keymap.
//!
//! One table, and a test that holds it to [`Action::ALL`]. The derive that
//! would make the list mechanical (`AllVariants`) is unit-only and pauta's
//! action enum is unit-only *for that reason* — but the binding table itself
//! still has to be authored, so the gate is a test rather than a type.

use egaku::{KeyCombo, KeyMap};

use crate::app::Action;

/// Every binding, in the order the legend shows them (most important first).
pub const BINDINGS: &[(&str, Action)] = &[
    ("j", Action::SelectNext),
    ("k", Action::SelectPrev),
    ("enter", Action::OpenDiff),
    ("esc", Action::CloseDiff),
    ("s", Action::ToggleDiffMode),
    ("tab", Action::CycleLane),
    ("a", Action::Approve),
    ("r", Action::RequestChanges),
    ("y", Action::Confirm),
    ("n", Action::Cancel),
    ("q", Action::Quit),
];

/// Build the keymap.
#[must_use]
pub fn keymap() -> KeyMap<Action> {
    let mut m = KeyMap::new();
    for (k, a) in BINDINGS {
        m.bind(KeyCombo::key(k), *a);
    }
    m
}

/// The legend, most important first — derived from [`BINDINGS`] so it cannot
/// advertise a chord the runtime does not bind. banken's hand-written legend
/// drifted into promising `S` for a chord bound as `shift+s`; deriving it is
/// the fix, and this is the same fix applied before the drift happens.
#[must_use]
pub fn legend_parts() -> Vec<String> {
    BINDINGS
        .iter()
        .map(|(k, a)| {
            let mut s = String::from(*k);
            s.push(':');
            s.push_str(a.slug());
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_binding_reaches_the_map() {
        let m = keymap();
        assert_eq!(
            m.len(),
            BINDINGS.len(),
            "a chord was swallowed on the way into the keymap"
        );
    }

    /// The gate that replaces the derive. If an `Action` variant is added and
    /// never bound, this fails — which is the drift `AllVariants` would have
    /// caught if it could be used on this enum.
    #[test]
    fn every_action_is_bound() {
        let bound: Vec<Action> = BINDINGS.iter().map(|(_, a)| *a).collect();
        for a in Action::ALL {
            assert!(
                bound.contains(a),
                "Action::{} has no key binding — unreachable from the keyboard",
                a.slug()
            );
        }
    }

    #[test]
    fn no_chord_is_bound_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for (k, _) in BINDINGS {
            assert!(
                seen.insert(*k),
                "chord `{k}` is bound twice — the second wins silently"
            );
        }
    }

    #[test]
    fn no_two_actions_share_a_slug() {
        let mut seen = std::collections::BTreeSet::new();
        for a in Action::ALL {
            assert!(seen.insert(a.slug()), "slug `{}` is used twice", a.slug());
        }
        assert_eq!(seen.len(), Action::ALL.len());
    }

    #[test]
    fn the_keymap_resolves_a_chord_to_its_action() {
        let m = keymap();
        let combo = KeyCombo::key("a");
        assert_eq!(m.lookup(&combo), Some(&Action::Approve));
    }

    #[test]
    fn the_legend_advertises_exactly_what_is_bound() {
        let legend = legend_parts();
        assert_eq!(legend.len(), BINDINGS.len());
        assert!(legend[0].starts_with("j:"), "legend order follows BINDINGS");
    }
}
