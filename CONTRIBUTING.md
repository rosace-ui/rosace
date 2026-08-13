# Contributing to Rosace

Rosace is not yet open for general contributions while the foundation is being
laid. Bug reports, ideas and focused PRs are still very welcome — open an issue
first so we can agree on scope before you spend time building.

Architectural decisions that govern the project live in
[`.steering/DECISIONS.md`](.steering/DECISIONS.md). Read it before opening a
PR: decisions marked `LOCKED` are not open for debate unless a new decision
supersedes them.

---

## AI-assisted contributions are welcome

Let's be clear about this, because plenty of projects are not.

**You may use AI to write your contribution.** This project was itself built
with AI assistance and says so openly on the front page. There is no stigma
here, no "human-written only" rule, and nobody is going to interrogate your
commit history looking for a machine. Used well, AI is a fast tool with a
human holding the judgement — which is exactly how Rosace is built.

What matters is not *how* the code was produced. It is whether the code is
correct, whether you understand it, and whether you can show your work.

So: use whatever tools you like, and meet the bar below.

### The bar

Every PR — AI-assisted or not — needs all four. AI raises the volume of code
a person can produce, and the review effort has to keep pace, so these are the
things that let a reviewer trust a change without re-deriving it from scratch.

**1. Say that you used AI.** One line in the PR description is enough:

> *Written with Claude / Copilot / Cursor / …, reviewed and tested by me.*

This is not a confession, it is context. It tells a reviewer where to look
harder — AI is very good at code that compiles and looks right, and less good
at knowing which invariant it just quietly broke. Disclosure is the one rule
here that is not negotiable: hiding it wastes a reviewer's time in a way that
is hard to forgive.

**2. Prove it works — one of:**

- **You tested it manually**, and the PR says *what you did and what you saw*.
  "Tested on macOS: opened the widget catalog, tapped every Chip variant,
  confirmed the selected state toggles and the toast fires" is a real report.
  "Tested, works" is not.
- **or the change carries test coverage above 80%** for the code it touches,
  with the tests exercising real behaviour rather than restating the
  implementation.

Both is better. If you cannot do either — say so, and say why; that is far
more useful than a claim that does not hold.

> A test that passes against the *unfixed* code proves nothing. When you fix a
> bug, revert your fix, watch the new test fail, then restore it. This project
> has caught more than one "fix" that way.

**3. Write a detailed report in the PR description.** Not a changelog line.
What problem you found, how you found it, what you changed, what you
deliberately did NOT change, and anything you are unsure about. Uncertainty
stated up front is a feature — it tells the reviewer where to spend attention.

If your change touches behaviour that a decision in `.steering/DECISIONS.md`
governs, name the decision.

**4. Screenshots for anything visual.** If you touch a widget, a layout, a
theme token, an animation or anything else a user can see:

- **before and after**, side by side
- **light and dark** if the change involves colour
- a **short screen recording** if it involves motion or gestures
- the **platform** you captured it on

A widget change without a screenshot cannot be reviewed, and will be sent back
for one. This is the single most common reason a UI PR stalls.

### What gets a PR rejected

Not the AI. These:

- Undisclosed AI use.
- "Tested, works" with nothing behind it.
- A widget or layout change with no screenshot.
- Code the author cannot explain. If a reviewer asks *why* a line is there and
  the answer is "that's what it generated", that is the problem — not the tool.
- Large unrequested refactors bundled into a fix. Keep PRs small and focused.
- Changes that contradict a `LOCKED` decision without superseding it.

---

## Practical checks before you open the PR

```sh
cargo test --workspace          # runs PARALLEL on purpose — see below
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

Tests run in parallel deliberately. They were pinned to `--test-threads=1`
once to paper over tests racing on shared global state, and that workaround
turned out to be hiding a real bug where two live engines dropped each other's
rebuilds — and the user's keystrokes with them. If your change introduces
shared mutable global state, parallel tests are what will tell you.

If you are touching a widget, read
[`.steering/WIDGET_QUALITY_BAR.md`](.steering/WIDGET_QUALITY_BAR.md) and
[`.steering/WIDGET_AUDIT_PIPELINE.md`](.steering/WIDGET_AUDIT_PIPELINE.md)
first. They are the checklist a widget is measured against: theme tokens
rather than hardcoded colours, OS text scaling, 44px touch targets, and
semantics for screen readers.

## Commit messages

Conventional-commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`,
`chore:`), with `!` for a breaking change. Explain **why**, not just what —
the diff already says what. Do not add AI co-author trailers; disclose in the
PR description instead.
