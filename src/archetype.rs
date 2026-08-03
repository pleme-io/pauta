//! The session archetype: a pull request becomes three panes.
//!
//! This is the edge that did not exist. izumi surfaces a PR on mado's Ctrl-S
//! board; `praca::instantiate` builds a multi-pane tear session from a
//! `SessionDefinition`; and there was **no path from one to the other**.
//! Everything on both sides was already shipped and tested; what was missing
//! was the twelve-line function in the middle.
//!
//! # The three panes
//!
//! | slot | what | why |
//! |---|---|---|
//! | 0 | `pauta` | the review itself — the docket, filtered to this PR |
//! | 1 | a shell | for the checkout, `git log`, running the tests |
//! | 2 | `claude`, primed | an agent that already knows which PR you are on |
//!
//! # Why the agent pane is argv, not keystrokes
//!
//! The obvious way to prime an agent is to spawn it and inject the prompt as
//! input. That path is closed, and closed for a good reason: tear's
//! `reject_injection` refuses a `\n` in injected text, and `with_command`
//! swallows the rejection silently — so the prompt would either not arrive or
//! arrive mangled, with nothing reporting it.
//!
//! `SpawnSpec::args` sidesteps it entirely. The prompt is argv, delivered by
//! the kernel at exec time. Nothing parses it, nothing can mangle it, and a
//! newline in it is just a byte. This is the load-bearing fix rather than a
//! workaround for the injection path — argv is what argv is for.
//!
//! # Why this module lives in pauta
//!
//! It needs `metsuke`'s coordinate types and it produces `praca`'s plan
//! types, and pauta already depends on the first. Putting it in praca would
//! make the multiplexer know about pull requests; putting it in metsuke would
//! make the review client know about panes. Neither should.

use std::collections::BTreeMap;
use std::path::PathBuf;

use metsuke::{PrNumber, RepoId};
use praca::{NameStyle, SessionDefinition, SessionOrigin, ThemeMirror};
use tear_types::layout::SplitRatio;
use tear_types::{DefinitionId, LayoutPlan, PaneSlot, SpawnSpec, SplitOrientation, WindowPlan};

/// The pauta pane.
pub const SLOT_REVIEW: PaneSlot = PaneSlot(0);
/// The shell pane.
pub const SLOT_SHELL: PaneSlot = PaneSlot(1);
/// The primed-agent pane.
pub const SLOT_AGENT: PaneSlot = PaneSlot(2);

/// What a PR session needs to know. Everything else is derived.
///
/// A struct rather than five positional arguments because `owner`, `name` and
/// the two paths are all strings, and a caller that swaps two of them gets a
/// session in the wrong directory with no compiler complaint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSession {
    /// Which pull request.
    pub repo: RepoId,
    pub number: PrNumber,
    /// The PR's title — used only for the session's display name.
    pub title: String,
    /// Where the checkout lives. Every pane opens here, which is why per-pane
    /// cwd is not on the critical path for this archetype: all three want the
    /// same directory.
    pub checkout: PathBuf,
    /// The shell binary for the middle pane.
    pub shell: String,
    /// The pauta binary for the review pane.
    pub pauta_bin: String,
    /// The agent binary for the primed pane.
    pub agent_bin: String,
}

impl PrSession {
    /// The stable identity of this PR's session, as izumi spells it.
    ///
    /// Byte-identical to `izumi::CorrKey::github` — `gh:owner/name#N` — and
    /// that is the point rather than a coincidence. izumi already emits that
    /// key from all three of its GitHub sources, and `collapse_correlated`
    /// already drops a row whose corr is live. Deriving the session's
    /// identity from the same string gives per-PR reuse for free:
    /// re-selecting a PR finds the existing session instead of spawning a
    /// second one.
    #[must_use]
    pub fn corr_key(&self) -> String {
        let mut s = String::from("gh:");
        s.push_str(&self.repo.owner);
        s.push('/');
        s.push_str(&self.repo.name);
        s.push('#');
        s.push_str(&self.number.0.to_string());
        s
    }

    /// The prompt the agent pane is spawned with.
    ///
    /// Deliberately states the PR and stops. It does not instruct the agent
    /// what to conclude — priming an agent with "review this and approve it"
    /// is how you get an approval you did not read.
    #[must_use]
    pub fn agent_prompt(&self) -> String {
        let mut p = String::from("I'm reviewing ");
        p.push_str(&self.repo.owner);
        p.push('/');
        p.push_str(&self.repo.name);
        p.push('#');
        p.push_str(&self.number.0.to_string());
        p.push_str(" — \"");
        p.push_str(&self.title);
        p.push_str("\". The checkout is in this directory. Help me understand ");
        p.push_str("what changed and why; don't submit any review.");
        p
    }
}

/// Build the three-pane definition.
///
/// Pure — no tear, no network, no filesystem. The caller hands the result to
/// `praca::instantiate`, which is the shipped, tested, validated path. This
/// deliberately does NOT add a second pane-making mechanism.
#[must_use]
pub fn pr_session_definition(s: &PrSession) -> SessionDefinition {
    let cwd = s.checkout.to_string_lossy().into_owned();

    let spec = |slot: PaneSlot, shell: &str, args: Vec<String>, title: &str| SpawnSpec {
        slot,
        shell: shell.to_owned(),
        args,
        cwd: Some(cwd.clone()),
        env: Vec::new(),
        title: title.to_owned(),
        input_policy: tear_types::InputPolicy::default(),
    };

    let mut pane_specs = BTreeMap::new();
    pane_specs.insert(
        SLOT_REVIEW,
        spec(SLOT_REVIEW, &s.pauta_bin, Vec::new(), "review"),
    );
    pane_specs.insert(SLOT_SHELL, spec(SLOT_SHELL, &s.shell, Vec::new(), "shell"));
    pane_specs.insert(
        SLOT_AGENT,
        // The prompt as ONE argv element. See the module docs: injection is
        // the path that mangles it.
        spec(SLOT_AGENT, &s.agent_bin, vec![s.agent_prompt()], "agent"),
    );

    // pauta left at 55%, shell over agent on the right. The review is the
    // thing you look at; the other two are where you act.
    let layout = LayoutPlan::Split {
        orientation: SplitOrientation::Vertical,
        ratio: SplitRatio::new(0.55),
        a: Box::new(LayoutPlan::Leaf { slot: SLOT_REVIEW }),
        b: Box::new(LayoutPlan::Split {
            orientation: SplitOrientation::Horizontal,
            ratio: SplitRatio::BALANCED,
            a: Box::new(LayoutPlan::Leaf { slot: SLOT_SHELL }),
            b: Box::new(LayoutPlan::Leaf { slot: SLOT_AGENT }),
        }),
    };

    // `DefinitionId` is a u64 newtype, so the corr key is hashed rather than
    // stored. Same fnv1a izumi uses, so the mapping PR -> id is stable across
    // processes, restarts and hosts — which is what makes reuse work at all.
    let seed = fnv1a(s.corr_key().as_bytes());

    // Built from `single_pane` and then widened, rather than struct-literalled:
    // that constructor is the sanctioned path, and going through it means a
    // new required field on `SessionDefinition` is a compile error here
    // instead of a silently-defaulted one.
    let mut def = SessionDefinition::single_pane(
        s.checkout.clone(),
        s.shell.clone(),
        NameStyle::default(),
        0,
    );
    def.def_id = DefinitionId(seed);
    def.name_seed = seed;
    // Adhoc, not Project: this session belongs to a PULL REQUEST, not to the
    // directory it happens to be checked out in. Two PRs on one checkout are
    // two sessions.
    def.origin = SessionOrigin::Adhoc {
        theme: ThemeMirror::default(),
        seed,
    };
    // The PR's coordinate IS the name — a docket of six sessions all called
    // after the same checkout directory would be useless.
    def.custom_name = Some(s.corr_key());
    def.windows = vec![WindowPlan {
        name: "review".to_owned(),
        layout,
        // The review pane has focus on open: reading comes before acting.
        active_slot: SLOT_REVIEW,
    }];
    def.pane_specs = pane_specs;
    def
}

/// Same fnv1a izumi derives its ids with, so a name seed for a PR is stable
/// across processes and restarts.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess() -> PrSession {
        PrSession {
            repo: RepoId::parse("pleme-io/metsuke").unwrap(),
            number: PrNumber(12),
            title: "add the thing".into(),
            checkout: PathBuf::from("/tmp/checkouts/metsuke-12"),
            shell: "/bin/zsh".into(),
            pauta_bin: "pauta".into(),
            agent_bin: "claude".into(),
        }
    }

    #[test]
    fn the_definition_id_matches_izumis_corr_key_byte_for_byte() {
        // This is what buys per-PR session reuse. If it drifts, re-selecting
        // a PR spawns a SECOND session instead of switching to the first, and
        // nothing reports it.
        assert_eq!(sess().corr_key(), "gh:pleme-io/metsuke#12");
    }

    #[test]
    fn all_three_panes_are_planned_and_land_in_the_checkout() {
        let d = pr_session_definition(&sess());
        assert_eq!(d.pane_specs.len(), 3, "review + shell + agent");
        for (slot, spec) in &d.pane_specs {
            assert_eq!(
                spec.cwd.as_deref(),
                Some("/tmp/checkouts/metsuke-12"),
                "slot {slot:?} must open in the checkout"
            );
        }
    }

    #[test]
    fn the_agent_prompt_is_one_argv_element_not_injected_text() {
        let d = pr_session_definition(&sess());
        let agent = &d.pane_specs[&SLOT_AGENT];
        assert_eq!(agent.shell, "claude");
        assert_eq!(
            agent.args.len(),
            1,
            "the whole prompt is ONE argument — splitting it on spaces is how \
             a prompt arrives as forty arguments"
        );
        assert!(agent.args[0].contains("pleme-io/metsuke#12"));
    }

    #[test]
    fn the_prompt_does_not_tell_the_agent_what_to_conclude() {
        let p = sess().agent_prompt();
        assert!(
            p.contains("don't submit any review"),
            "an agent primed to approve produces approvals nobody read: {p:?}"
        );
        for verdict in ["approve it", "lgtm", "looks good"] {
            assert!(!p.to_lowercase().contains(verdict), "{p:?}");
        }
    }

    #[test]
    fn the_review_pane_has_focus_on_open() {
        let d = pr_session_definition(&sess());
        assert_eq!(
            d.windows[0].active_slot, SLOT_REVIEW,
            "reading comes before acting"
        );
    }

    #[test]
    fn the_definition_validates_against_pracas_own_rules() {
        // The real gate: praca refuses a malformed definition, and this is
        // the same `validate()` that `instantiate` calls first. A definition
        // that only satisfies the assertions above is not proven usable.
        let d = pr_session_definition(&sess());
        assert!(
            d.validate().is_ok(),
            "praca rejected the archetype: {:?}",
            d.validate().err()
        );
    }

    #[test]
    fn every_layout_leaf_has_a_spec_and_every_spec_a_leaf() {
        // The join between LayoutPlan and pane_specs is by slot, and a
        // mismatch either leaves a pane unspawned or a spec unused. Both are
        // silent.
        let d = pr_session_definition(&sess());
        let mut leaves = Vec::new();
        collect(&d.windows[0].layout, &mut leaves);
        leaves.sort_unstable();
        let mut specs: Vec<PaneSlot> = d.pane_specs.keys().copied().collect();
        specs.sort_unstable();
        assert_eq!(leaves, specs, "layout slots and pane specs must agree");
    }

    fn collect(l: &LayoutPlan, out: &mut Vec<PaneSlot>) {
        match l {
            LayoutPlan::Leaf { slot } => out.push(*slot),
            LayoutPlan::Split { a, b, .. } => {
                collect(a, out);
                collect(b, out);
            }
        }
    }

    #[test]
    fn two_prs_get_two_definitions_and_one_pr_gets_one() {
        let a = sess();
        let mut b = sess();
        b.number = PrNumber(13);
        assert_ne!(
            pr_session_definition(&a).def_id,
            pr_session_definition(&b).def_id
        );
        assert_eq!(
            pr_session_definition(&a).def_id,
            pr_session_definition(&sess()).def_id,
            "the same PR must resolve to the same session, or reuse is broken"
        );
    }
}

/// Instantiating the archetype end to end, against praça's own mock backend.
///
/// Separate from the unit tests above because these prove a different thing:
/// not that the definition is *shaped* right, but that `praca::instantiate`
/// actually drives it into three panes. A definition that validates and then
/// spawns one pane would pass every test above.
#[cfg(test)]
mod instantiate_tests {
    use super::*;
    use praca::instantiate;
    use praca::mock_backend::{Call, MockBackend};

    fn sess() -> PrSession {
        PrSession {
            repo: RepoId::parse("pleme-io/metsuke").unwrap(),
            number: PrNumber(12),
            title: "add the thing".into(),
            checkout: PathBuf::from("/tmp/checkouts/metsuke-12"),
            shell: "/bin/zsh".into(),
            pauta_bin: "pauta".into(),
            agent_bin: "claude".into(),
        }
    }

    #[test]
    fn instantiating_the_archetype_spawns_exactly_three_panes() {
        let backend = MockBackend::new();
        let def = pr_session_definition(&sess());
        let live = instantiate(&def, &backend).expect("praca must instantiate the archetype");

        let calls = backend.calls();
        let sessions = calls
            .iter()
            .filter(|c| matches!(c, Call::NewSession { .. }))
            .count();
        let splits = calls
            .iter()
            .filter(|c| matches!(c, Call::SplitPane { .. }))
            .count();
        assert_eq!(sessions, 1, "one session");
        assert_eq!(
            splits, 2,
            "three panes = one session pane + two splits; got {calls:#?}"
        );
        assert_eq!(
            live.session.panes.len(),
            3,
            "three live panes in the instantiated session"
        );
    }

    /// The agent's prompt survives the whole path — definition, instantiate,
    /// backend — as ONE argument. This is the assertion the argv decision
    /// exists for: it is the first point where a keystroke-injection design
    /// would have lost or mangled it.
    #[test]
    fn the_agent_prompt_reaches_the_backend_intact() {
        let backend = MockBackend::new();
        let def = pr_session_definition(&sess());
        instantiate(&def, &backend).unwrap();

        let want = sess().agent_prompt();
        let found = backend.calls().iter().any(|c| match c {
            Call::SplitPane { shell, args, .. } | Call::NewSession { shell, args, .. } => {
                shell == "claude" && args.len() == 1 && args[0] == want
            }
            _ => false,
        });
        assert!(
            found,
            "the agent pane must reach the backend as `claude <one-arg-prompt>`: {:#?}",
            backend.calls()
        );
    }

    #[test]
    fn every_slot_becomes_a_distinct_live_pane_linked_to_its_definition() {
        let backend = MockBackend::new();
        let def = pr_session_definition(&sess());
        let live = instantiate(&def, &backend).unwrap();
        // Three DISTINCT panes: a layout that mapped two slots onto one pane
        // would still report a plausible count everywhere else.
        let mut ids: Vec<_> = live.session.panes.keys().collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            3,
            "three slots must become three distinct panes: {:?}",
            live.session.panes
        );
        assert_eq!(
            live.definition, def.def_id,
            "the live session must point back at the definition that made it — \
             that link is how re-selecting the same PR finds this session"
        );
    }
}
