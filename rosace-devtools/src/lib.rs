//! Developer tools for ROSACE.
//!
//! Provides a trace viewer, atom state inspector, frame profiler, and
//! a combined dev console that aggregates all three.
//!
//! # Example
//! ```rust,ignore
//! use rosace_devtools::DevConsole;
//! let mut console = DevConsole::new();
//! console.profiler.begin_frame();
//! println!("{}", console.render());
//! ```

pub mod atom_inspector;
pub mod component_inspector;
pub mod dev_console;
pub mod element_inspector;
pub mod frame_profiler;
pub mod devtools_ui;
pub mod trace_panel;
pub mod trace_viewer;

pub use atom_inspector::{atom_entry, AtomEntry, AtomInspector, AtomSnapshot};
pub use component_inspector::{ComponentInspector, LayoutNode};
pub use dev_console::DevConsole;
pub use element_inspector::{node_rect, panel_lines, to_layout_tree, ElementInspector};
pub use frame_profiler::FrameProfiler;
pub use trace_panel::TracePanel;
pub use devtools_ui::{devtools_overlay, DEVTOOLS_OPEN, DEVTOOLS_TAB};
pub use trace_viewer::TraceViewer;
