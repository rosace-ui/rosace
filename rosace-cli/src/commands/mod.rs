/// `rsc analyze` — workspace health analytics.
pub mod analyze;
/// `rsc bundle-id [<id>]` — read or update the app's bundle identifier.
pub mod bundle_id;
/// `rsc build` — release build for a target platform.
pub mod build;
/// `rsc dev` — development server with terminal trace output.
pub mod dev;
pub mod hot_ws;
/// `rsc devices` — list available run targets across platforms.
pub mod devices;
/// `rsc doctor` — environment diagnostics for every platform's toolchain.
pub mod doctor;
/// App icon generation for `rsc new` — bundled SVG rasterized per platform.
pub mod icons;
/// `rsc new` — scaffold a new ROSACE app project.
pub mod new;
/// `rsc package` — bundle for distribution (.app / .deb / .exe).
pub mod package;
/// `rsc run` — build + run the current app on desktop / web / iOS.
pub mod run;
/// `rsc snapshot` — run an example and save its PNG output.
pub mod snapshot;
pub mod tier2;
/// `rsc check`, `rsc test`, `rsc lint`, `rsc fmt` — workspace quality commands.
pub mod workspace;
