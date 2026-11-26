//! The board's palette: the one `waybill_core::theme` works out, held for
//! the thread that draws.

use std::cell::RefCell;
pub use waybill_core::theme::{Palette, Rgba};

thread_local! {
    static CURRENT: RefCell<Palette> = RefCell::new(Palette::default());
}

pub fn current() -> Palette {
    CURRENT.with(|p| p.borrow().clone())
}

pub fn set_current(p: Palette) {
    CURRENT.with(|c| *c.borrow_mut() = p);
}
