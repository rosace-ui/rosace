/// `tzr analyze` — workspace health analytics.
pub mod analyze;
/// `tzr bundle-id [<id>]` — read or update the app's bundle identifier.
pub mod bundle_id;
/// `tzr build` — release build for a target platform.
pub mod build;
/// `tzr dev` — development server with terminal trace output.
pub mod dev;
/// App icon generation for `tzr new` — bundled SVG rasterized per platform.
pub mod icons;
/// `tzr new` — scaffold a new TEZZERA app project.
pub mod new;
/// `tzr package` — bundle for distribution (.app / .deb / .exe).
pub mod package;
/// `tzr run` — build + run the current app on desktop / web / iOS.
pub mod run;
/// `tzr snapshot` — run an example and save its PNG output.
pub mod snapshot;
/// `tzr check`, `tzr test`, `tzr lint`, `tzr fmt` — workspace quality commands.
pub mod workspace;
