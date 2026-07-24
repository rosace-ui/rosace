//! Template descriptor (D103 / D102 Tier 1) — the **data** form of a `view!`
//! tree, and the runtime that inflates it back into widgets.
//!
//! This is the contract at the centre of universal (data-not-code) hot reload:
//! the `view!` macro *writes* a [`Template`] in dev builds, and a runtime
//! interpreter *reads* it to reconstruct the widget subtree by calling the same
//! widget constructors the release builder would (the "inflater, not a
//! renderer" model — see `.steering/HOT_RELOAD.md`). Because it is pure data —
//! widget kinds by **name**, literal props as data, dynamic `{expr}` bits as
//! numbered **holes** — a template can be diffed and swapped at runtime without
//! recompiling, which is the only reload path that works on iOS device and web.
//!
//! # Layers built on this
//! - Step 2 (this file): the descriptor data model — [`Template`],
//!   [`TemplateNode`], [`PropValue`], [`StaticValue`], [`TemplateKey`].
//! - Step 3 (next): the widget registry + interpreter that turn a [`Template`]
//!   into `Box<dyn Widget>` — that is where `RosaceTrace` inflate events are
//!   emitted (this data model is inert, so it emits none itself).
//! - Step 4: the dev watcher diffs templates by [`TemplateKey`] and pushes
//!   deltas over the control channel.
//!
//! # Wire form (named-deferred)
//! The types are deliberately plain data — `String` widget/prop names, a
//! primitive-only [`StaticValue`], no borrowed lifetimes — so a
//! `#[derive(Serialize, Deserialize)]` drops on cleanly the day the SDUI /
//! external-DevTools transport needs a JSON form (the plan's "design it so it
//! is not foreclosed"). `serde` is **not** pulled into the workspace here — it
//! is a named deferral to the transport step (rollout step 4), consistent with
//! D121 treating a new dependency as its own decision rather than smuggling it
//! in with an unrelated change.

mod descriptor;
mod diff;
mod inflate;
mod parse;
mod registry;
mod reload;
mod swap;

pub use descriptor::{PropValue, StaticValue, Template, TemplateKey, TemplateNode};
pub use diff::{diff, EscalationReason, TemplateDiff};
pub use inflate::{inflate, is_registered, register_widget, FromProp, Handler, InflateError, PropInput};
pub use parse::{parse_file_templates, parse_template, ParseError};
pub use registry::{find_running, get, len, register, snapshot};
pub use reload::{apply_reload, ReloadReport};
pub use swap::{apply_swap, SwapOutcome};
