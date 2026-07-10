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
pub mod frame_profiler;
pub mod trace_viewer;

pub use atom_inspector::{atom_entry, AtomEntry, AtomInspector, AtomSnapshot};
pub use component_inspector::{ComponentInspector, LayoutNode};
pub use dev_console::DevConsole;
pub use frame_profiler::FrameProfiler;
pub use trace_viewer::TraceViewer;
