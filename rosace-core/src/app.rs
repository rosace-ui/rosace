use crate::element::Element;

/// Top-level application builder.
///
/// Holds the root element and (in future phases) the platform event-loop
/// handle, window configuration, and service registrations.
pub struct App {
    root: Option<Element>,
}

impl App {
    /// Creates a new `App` with no root element.
    pub fn new() -> Self {
        App { root: None }
    }

    /// Sets the root element of the application.
    pub fn child(mut self, element: impl Into<Element>) -> Self {
        self.root = Some(element.into());
        self
    }

    /// Starts the application event loop.
    ///
    /// # Phase 1 placeholder
    ///
    /// The real event loop is wired in `rosace-render` (GPU path) and
    /// `rosace-cli` (terminal path). This stub exists so application entry
    /// points compile against `rosace-core` without pulling in renderer crates.
    pub fn run(self) {
        todo!("wire up in rosace-render/rosace-cli")
    }
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}
