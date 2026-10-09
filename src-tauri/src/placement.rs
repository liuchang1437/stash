//! Where the popover goes: next to the caret of the window the user was
//! typing in, or next to the mouse when the caret is unknown.
//!
//! All rectangles are physical screen pixels; sizes coming from the UI are
//! logical (CSS) pixels and get multiplied by the monitor's scale factor.

use serde::{Deserialize, Serialize};

use crate::platform::Rect;

/// Gap between the caret line and the popover card, logical px.
const GAP: f64 = 4.0;
/// Horizontal offset from the mouse pointer when there is no caret.
const MOUSE_OFFSET: i32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorKind {
    Caret,
    Mouse,
    /// Fixed spot in the upper middle of the screen.
    Center,
}

#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub kind: AnchorKind,
    /// Left edge the popover card lines up with.
    pub x: i32,
    /// Top and bottom of the caret line (equal for a mouse anchor).
    pub top: i32,
    pub bottom: i32,
    /// Work area of the monitor the anchor is on.
    pub work: Rect,
    pub scale: f64,
}

/// Free space around the anchor, logical px. The UI uses it to decide
/// whether to open upwards and on which side the preview goes.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Space {
    pub above: f64,
    pub below: f64,
    pub left: f64,
    pub right: f64,
}

/// Window geometry requested by the UI, logical px. The window is
/// transparent: `margin` is the shadow border around the cards, and the
/// main card's left edge sits `card_x` from the window's left edge (more
/// than `margin` when a side panel opens to the left of it).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub card_x: f64,
    pub margin: f64,
    /// Open upwards: the card's bottom edge sits above the caret line.
    pub flip: bool,
}

impl Anchor {
    pub fn caret(caret: Rect, work: Rect, scale: f64) -> Self {
        Anchor {
            kind: AnchorKind::Caret,
            x: caret.left,
            top: caret.top,
            bottom: caret.bottom,
            work,
            scale,
        }
    }

    pub fn mouse(x: i32, y: i32, work: Rect, scale: f64) -> Self {
        Anchor {
            kind: AnchorKind::Mouse,
            x: x + MOUSE_OFFSET,
            top: y,
            bottom: y,
            work,
            scale,
        }
    }

    /// Upper middle of the work area: content `width` logical px wide is
    /// centered horizontally, its top a fifth of the way down.
    pub fn centered(work: Rect, scale: f64, width: f64) -> Self {
        let w = (width * scale).round() as i32;
        let x = work.left + ((work.right - work.left) - w).max(0) / 2;
        let y = work.top + (work.bottom - work.top) / 5;
        Anchor {
            kind: AnchorKind::Center,
            x,
            top: y,
            bottom: y,
            work,
            scale,
        }
    }

    pub fn space(&self) -> Space {
        let s = self.scale;
        Space {
            above: (self.top - self.work.top) as f64 / s,
            below: (self.work.bottom - self.bottom) as f64 / s,
            left: (self.x - self.work.left) as f64 / s,
            right: (self.work.right - self.x) as f64 / s,
        }
    }

    /// Window rectangle (x, y, width, height) in physical px, kept inside
    /// the work area.
    pub fn place(&self, layout: &Layout) -> (i32, i32, i32, i32) {
        let s = self.scale;
        let w = (layout.width * s).round() as i32;
        let h = (layout.height * s).round() as i32;
        let gap = (GAP * s).round() as i32;
        let margin = (layout.margin * s).round() as i32;
        let x = self.x - (layout.card_x * s).round() as i32;
        let y = if layout.flip {
            self.top - gap + margin - h
        } else {
            self.bottom + gap - margin
        };
        let work = self.work;
        let x = x.min(work.right - w).max(work.left);
        let y = y.min(work.bottom - h).max(work.top);
        (x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn layout(flip: bool) -> Layout {
        Layout {
            width: 480.0,
            height: 300.0,
            card_x: 20.0,
            margin: 20.0,
            flip,
        }
    }

    #[test]
    fn opens_below_the_caret_aligned_with_it() {
        let a = Anchor::caret(
            Rect {
                left: 300,
                top: 200,
                right: 301,
                bottom: 220,
            },
            WORK,
            1.0,
        );
        assert_eq!(a.place(&layout(false)), (280, 204, 480, 300));
    }

    #[test]
    fn flips_above_the_caret() {
        let a = Anchor::caret(
            Rect {
                left: 300,
                top: 900,
                right: 301,
                bottom: 920,
            },
            WORK,
            1.0,
        );
        let (_, y, _, h) = a.place(&layout(true));
        // Card bottom (window bottom minus margin) sits GAP above the caret.
        assert_eq!(y + h - 20, 896);
    }

    #[test]
    fn scales_and_clamps_to_the_work_area() {
        let a = Anchor::caret(
            Rect {
                left: 1900,
                top: 1000,
                right: 1901,
                bottom: 1030,
            },
            WORK,
            1.5,
        );
        let (x, y, w, h) = a.place(&layout(false));
        assert_eq!((w, h), (720, 450));
        assert_eq!(x, 1920 - 720);
        assert_eq!(y, 1040 - 450);
    }

    #[test]
    fn centers_in_the_upper_middle() {
        let a = Anchor::centered(WORK, 2.0, 460.0);
        // 920 physical px wide, centered in 1920; top at 1040 / 5.
        assert_eq!((a.x, a.top, a.bottom), (500, 208, 208));
        // Opening down from there, the card's top edge is GAP below the anchor.
        let (x, y, _, _) = a.place(&layout(false));
        assert_eq!((x, y), (500 - 40, 208 + 8 - 40));
    }

    #[test]
    fn reports_space_in_logical_pixels() {
        let a = Anchor::mouse(586, 400, WORK, 2.0);
        let s = a.space();
        assert_eq!(
            (s.left, s.right, s.above, s.below),
            (300.0, 660.0, 200.0, 320.0)
        );
    }
}
