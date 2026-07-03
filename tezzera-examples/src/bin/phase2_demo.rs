//! Phase 2 demo — theme gallery, widget showcase, animation lab, scrolling feed.
//!
//! Navigation is driven by a plain `Atom<Screen>` enum.

use tezzera::prelude::*;
use tezzera::animate::use_spring;

// ---------------------------------------------------------------------------
// Screen enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    ThemeGallery,
    WidgetShowcase,
    AnimationLab,
    ScrollingFeed,
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

struct Phase2Demo;

impl Component for Phase2Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let screen  = ctx.state(Screen::ThemeGallery);
        let is_dark = ctx.state(true);

        // Spring animation state (Animation Lab screen)
        let anim_target = ctx.state(0.0f32);
        let (anim_pos, anim_ctrl) = use_spring(ctx, 0.0);
        anim_ctrl.animate_to(anim_target.get());

        // Theme toggle
        let theme = if is_dark.get() { dark_theme() } else { light_theme() };
        tezzera::theme::set_theme(theme.clone());

        let cur = screen.get();

        // ── Tab bar ──────────────────────────────────────────────────────────
        let tabs = {
            let screens = [
                (Screen::ThemeGallery,   "Theme"),
                (Screen::WidgetShowcase, "Widgets"),
                (Screen::AnimationLab,   "Animation"),
                (Screen::ScrollingFeed,  "Feed"),
            ];
            let mut row = Row::new()
                .spacing(4.0)
                .main_axis_alignment(MainAxisAlignment::Center);
            for (s, label) in screens {
                let sc = screen.clone();
                let active = s == cur;
                row = row.child(
                    Button::new(label)
                        .variant(if active { ButtonVariant::Primary } else { ButtonVariant::Ghost })
                        .width(90.0)
                        .on_press(move || sc.set(s)),
                );
            }
            row
        };

        // ── Screen body ──────────────────────────────────────────────────────
        let body: Box<dyn Widget> = match cur {
            Screen::ThemeGallery => {
                let is_dark2 = is_dark.clone();
                let dark = is_dark.get();
                let c = &theme.colors;
                let swatch = |label: &'static str, r: u8, g: u8, b: u8| -> Box<dyn Widget> {
                    Box::new(
                        Row::new()
                            .spacing(8.0)
                            .child(Container::new().background(Color::rgba(r, g, b, 255)).size(32.0, 24.0))
                            .child(Text::new(label))
                    )
                };
                Box::new(
                    Column::new()
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new("Color tokens — toggle theme above").align(TextAlign::Center))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(
                            Row::new()
                                .spacing(8.0)
                                .main_axis_alignment(MainAxisAlignment::Center)
                                .child(Text::new(if dark { "Dark" } else { "Light" }))
                                .child(Button::new("Toggle theme").on_press(move || is_dark2.set(!is_dark2.get())))
                        )
                        .child(Spacer::gap(0.0, 12.0))
                        .child(swatch("primary",    (c.primary.r * 255.0) as u8, (c.primary.g * 255.0) as u8, (c.primary.b * 255.0) as u8))
                        .child(swatch("secondary",  (c.secondary.r * 255.0) as u8, (c.secondary.g * 255.0) as u8, (c.secondary.b * 255.0) as u8))
                        .child(swatch("background", (c.background.r * 255.0) as u8, (c.background.g * 255.0) as u8, (c.background.b * 255.0) as u8))
                        .child(swatch("surface",    (c.surface.r * 255.0) as u8, (c.surface.g * 255.0) as u8, (c.surface.b * 255.0) as u8))
                        .child(swatch("error",      (c.error.r * 255.0) as u8, (c.error.g * 255.0) as u8, (c.error.b * 255.0) as u8))
                )
            }

            Screen::WidgetShowcase => {
                Box::new(
                    Column::new()
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new("Button variants:"))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(
                            Row::new()
                                .spacing(8.0)
                                .child(Button::new("Primary").variant(ButtonVariant::Primary).on_press(|| {}))
                                .child(Button::new("Secondary").variant(ButtonVariant::Secondary).on_press(|| {}))
                                .child(Button::new("Ghost").variant(ButtonVariant::Ghost).on_press(|| {}))
                                .child(Button::new("Danger").variant(ButtonVariant::Danger).on_press(|| {}))
                        )
                        .child(Spacer::gap(0.0, 16.0))
                        .child(Text::new("Divider:"))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Divider::horizontal())
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new("Disabled button:"))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Button::new("Disabled").disabled().on_press(|| {}))
                )
            }

            Screen::AnimationLab => {
                let bar_w = anim_pos.get().abs().min(320.0);
                let tgt = anim_target.clone();
                let tgt2 = anim_target.clone();
                let ctrl_snap = anim_ctrl.clone();
                let tgt3 = anim_target.clone();
                Box::new(
                    Column::new()
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new("Spring Animation Lab"))
                        .child(Spacer::gap(0.0, 12.0))
                        .child(
                            Container::new().background(Color::rgba(103, 80, 164, 255))
                                .size(bar_w.max(4.0), 36.0)
                        )
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new(format!("position: {:.1} / 320", anim_pos.get())))
                        .child(Spacer::gap(0.0, 12.0))
                        .child(
                            Row::new()
                                .spacing(8.0)
                                .child(Button::new("Animate →").on_press(move || tgt.set(320.0)))
                                .child(Button::new("← Back").variant(ButtonVariant::Secondary).on_press(move || tgt2.set(0.0)))
                                .child(
                                    Button::new("Snap to 0")
                                        .variant(ButtonVariant::Ghost)
                                        .on_press(move || {
                                            tgt3.set(0.0);
                                            ctrl_snap.snap_to(0.0);
                                        })
                                )
                        )
                )
            }

            Screen::ScrollingFeed => {
                let mut col = Column::new()
                    .child(Spacer::gap(0.0, 8.0))
                    .child(Text::new("Scrollable feed — 30 items:"))
                    .child(Spacer::gap(0.0, 8.0));
                for i in 1..=30 {
                    col = col
                        .child(Text::new(format!("Item #{i} — Lorem ipsum dolor sit amet")))
                        .child(Spacer::gap(0.0, 2.0))
                        .child(Divider::horizontal())
                        .child(Spacer::gap(0.0, 6.0));
                }
                Box::new(col)
            }
        };

        // ── Assemble ─────────────────────────────────────────────────────────
        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 8.0))
                .child(tabs)
                .child(Spacer::gap(0.0, 8.0))
                .child(body)
        )
        .app_bar(AppBar::new("TEZZERA — Phase 2 Demo"))
        .into_element()
    }
}

fn main() {
    App::new()
        .title("TEZZERA Phase 2 Demo")
        .size(680, 540)
        .dark()
        .launch(Phase2Demo);
}
