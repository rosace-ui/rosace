use crate::atom_inspector::AtomInspector;
use crate::component_inspector::ComponentInspector;
use crate::frame_profiler::FrameProfiler;
use crate::trace_viewer::TraceViewer;

/// Aggregates all dev tools into a single console.
pub struct DevConsole {
    pub trace:     TraceViewer,
    pub atoms:     AtomInspector,
    pub profiler:  FrameProfiler,
    pub layout:    ComponentInspector,
    pub enabled:   bool,
}

impl DevConsole {
    pub fn new() -> Self {
        Self {
            trace:    TraceViewer::new(),
            atoms:    AtomInspector::new(),
            profiler: FrameProfiler::new(),
            layout:   ComponentInspector::new(),
            enabled:  true,
        }
    }

    /// Render all panels as a combined ASCII summary.
    pub fn render(&self) -> String {
        if !self.enabled {
            return String::new();
        }
        let mut out = String::new();
        out.push_str(&self.profiler.render());
        out.push_str(&self.trace.render());
        out.push_str(&self.atoms.render_at_cursor());
        out.push_str(&self.layout.render());
        out
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
}

impl Default for DevConsole {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_console_renders_non_empty_when_enabled() {
        let console = DevConsole::new();
        assert!(console.enabled);
        let output = console.render();
        assert!(!output.is_empty());
    }

    #[test]
    fn dev_console_renders_empty_when_disabled() {
        let mut console = DevConsole::new();
        console.enabled = false;
        assert_eq!(console.render(), "");
    }

    #[test]
    fn dev_console_toggle_flips_enabled() {
        let mut console = DevConsole::new();
        assert!(console.enabled);
        console.toggle();
        assert!(!console.enabled);
        console.toggle();
        assert!(console.enabled);
    }

    #[test]
    fn dev_console_contains_all_panels() {
        let console = DevConsole::new();
        let output = console.render();
        // FPS panel header
        assert!(output.contains("FPS"), "missing FPS panel");
        // Trace panel: either event log or no-events placeholder
        assert!(
            output.contains("TEZZERA TRACE") || output.contains("No events yet"),
            "missing trace panel"
        );
        // Atom panel: either table or no-snapshots placeholder
        assert!(
            output.contains("ATOMS") || output.contains("No snapshots yet"),
            "missing atoms panel"
        );
    }
}
