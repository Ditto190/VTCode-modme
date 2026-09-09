//! Render scheduling state for a terminal session.

/// Separates redraw and destructive clear requests from the session's data.
#[derive(Debug)]
pub(super) struct RenderState {
    redraw_requested: bool,
    full_clear_requested: bool,
}

impl RenderState {
    pub(super) fn new() -> Self {
        Self {
            redraw_requested: true,
            full_clear_requested: false,
        }
    }

    pub(super) fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub(super) fn take_redraw(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
    }

    pub(super) fn request_full_clear(&mut self) {
        self.full_clear_requested = true;
        self.request_redraw();
    }

    pub(super) fn take_full_clear(&mut self) -> bool {
        std::mem::take(&mut self.full_clear_requested)
    }
}
