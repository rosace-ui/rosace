use crate::draw_command::DrawCommand;

/// An immutable, ordered list of [`DrawCommand`]s captured during one paint pass.
///
/// A `Picture` can be replayed many times (scrolling, animations) without calling
/// any widget's `paint()` method again — this is the foundation of the cached
/// repaint model.
#[derive(Clone)]
pub struct Picture {
    pub commands: Vec<DrawCommand>,
}

/// Accumulates [`DrawCommand`]s during the paint pass, then produces a [`Picture`].
///
/// `PaintCtx` holds a `&mut PictureRecorder`. Every drawing helper on `PaintCtx`
/// pushes a command here instead of writing pixels. After the full tree has been
/// painted, call [`finish`] to seal the recording.
pub struct PictureRecorder {
    commands: Vec<DrawCommand>,
}

impl PictureRecorder {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    #[inline]
    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    pub fn finish(self) -> Picture {
        Picture { commands: self.commands }
    }
}

impl Default for PictureRecorder {
    fn default() -> Self { Self::new() }
}
