//! One file per screen. Re-export each screen's builder here.

mod buttons;
mod counter;
mod gallery;
mod hero_detail;
mod home;
mod inputs;
mod overlays;
mod scroll_demo;

pub use buttons::buttons_screen;
pub use counter::counter_screen;
pub use gallery::gallery_screen;
pub use hero_detail::hero_detail_screen;
pub use home::home_screen;
pub use inputs::inputs_screen;
pub use overlays::overlays_screen;
pub use scroll_demo::scroll_screen;
