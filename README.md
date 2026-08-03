# pauta

**The PR-review quadro.** Your *docket*: every pull request set down for your
judgment, in order.

*Pauta* is Brazilian-Portuguese for the docket — the ordered list of matters a
session will deal with. That is the main screen.

```
pauta  --review-requested=@me
REPO                     PR      TITLE                    AUTHOR   STATE  UPDATED
> pleme-io/metsuke       #12     add the thing            drzzln   open   2026-08-03T…
  pleme-io/banken        #7      [draft] wip              drzzln   open   2026-08-02T…
source: LIVE  2 prs  j:select-next k:select-prev enter:open-diff a:approve! r:request-changes!
```

## What it is

A terminal UI over two seams from [`metsuke`](https://github.com/pleme-io/metsuke):

- `Forge` — where pull requests, diffs and threads come from.
- `ForgeReview` — where a verdict goes.

They are independent, so the runtime is identical against a mock, against a live
`gh`, and against a build with neither.

## Three things it does deliberately

**The docket refreshes itself** (QUADRO T14), adopted from the first commit
rather than retrofitted. The read runs on tokio's blocking pool via
`izumi::refresh`, never on the task that owns the terminal — a `gh` call there
would freeze input for its whole duration, and a slow forge is exactly when the
keyboard matters most. A faulted feed says so in the status line rather than
letting last-known rows read as current.

**A binding verdict takes two keystrokes.** Approving notifies the author and
changes whether the PR can merge, so the first chord *stages and previews* the
typed plan and a second commits. `commit_pending` is the only path that reaches
`submit_review`. The gate is read from the authored catalog, not decided in the
runtime — and `pauta-spec` gives an author no way to express "binding but
unconfirmed".

**One authored vocabulary.** The keymap, the legend and the gate all derive from
a single `(defpautaaction)` catalog in
[`pauta-spec`](./pauta-spec). Hand-written lists drift: banken's legend once
advertised `S` for a chord the runtime bound as `shift+s`. Here there is nothing
to drift from.

## What it does not do

No diff parsing. `egaku::DiffView` and `egaku::chigai` already own that, and a
second parser here would be the duplication `metsuke` was scoped narrowly to
avoid. pauta re-joins metsuke's per-file patches into one unified diff and hands
it over.

No forge mutation beyond a review. `metsuke`'s traits have no `merge`, no
`close`, no `push` — a reviewer reviews.

## Sessions

`pauta::archetype` turns a pull request into a three-pane tear session — the
review, a shell, and an agent primed with the PR context — handed to
`praca::instantiate`. The session's identity is izumi's own `gh:owner/name#N`,
so re-selecting a PR finds the existing session instead of spawning a second.

The agent's prompt is passed as **argv**, not injected keystrokes: tear's
`reject_injection` refuses a `\n` and `with_command` swallows the rejection
silently, so injection would deliver the prompt mangled with nothing reporting
it. It states the PR and stops — priming an agent with "review this and approve
it" is how you get an approval nobody read.

## Tier honesty

**Shipped:** the docket model, the live plane, the verdict gate, the diff panel,
the authored catalog, and the session archetype. 47 tests, no network; every
screen asserted **by position** over `TestBackend` rather than by searching a
blob. The verdict gate is verified non-vacuously — authoring `approve` as
`:gate free` turns three tests red.

The read path is proven against real GitHub through metsuke. The **write** path
is not: `submit_review` posts a real review to a real pull request, so it is
exercised against `MockForge` only. Nobody should approve someone's work to
satisfy a test suite. `pending-metsuke: live-write`.

Still authored in Rust rather than lisp: the column set and the sort default,
which want a `(defpautaview)`. `pending-pauta: view-catalog`.

## License

MIT
