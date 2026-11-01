//! Video Format: the contextual tab for a selected video. Crop it, turn or mirror it (Arrange ›
//! Rotate), and size it, as on Picture Format.

use super::arrange::{arrange_group, size_group};
use super::picture_format::crop_group;
use super::{SlideApp, Ui};

/// Render the Video Format tab.
pub(super) fn video_format(app: &mut SlideApp, ui: &mut Ui) {
    arrange_group(app, ui);
    crop_group(app, ui, false);
    size_group(app, ui);
}
