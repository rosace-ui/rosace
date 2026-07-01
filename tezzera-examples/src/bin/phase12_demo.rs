use std::time::Duration;
use tezzera::prelude::*;
use tezzera_animate::{use_spring, use_animation, SpringController, AnimCtrl};
use tezzera_state::Atom;

#[derive(Debug, Clone, PartialEq)]
enum Panel { VSync, LayoutCtx, TextMetrics, Animation, Overlay }

struct Phase12Demo;

impl Component for Phase12Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let panel: Atom<Panel> = ctx.state(Panel::VSync);
        let counter: Atom<i32> = ctx.state(0_i32);

        let p1 = panel.clone(); let p2 = panel.clone();
        let p3 = panel.clone(); let p4 = panel.clone();
        let p5 = panel.clone();
        let nav = Row::new()
            .spacing(8.0)
            .child(Button::new("VSync").on_press(move || p1.set(Panel::VSync)))
            .child(Button::new("LayoutCtx").on_press(move || p2.set(Panel::LayoutCtx)))
            .child(Button::new("Text").on_press(move || p3.set(Panel::TextMetrics)))
            .child(Button::new("Animation").on_press(move || p4.set(Panel::Animation)))
            .child(Button::new("Overlay").on_press(move || p5.set(Panel::Overlay)));

        let body: Box<dyn Widget> = match panel.get() {
            Panel::VSync => {
                let inc = counter.clone();
                let dec = counter.clone();
                let cnt = counter.get();
                Box::new(
                    Column::new().spacing(16.0)
                        .child(nav)
                        .child(Text::display("VSync Frame Scheduler"))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("D054 — Zero Idle CPU"))
                                .child(Text::caption("ControlFlow::Wait — event loop sleeps until a frame is needed"))
                                .child(Text::caption("Atom::set() → request_frame() → EventLoopProxy::send_event()"))
                                .child(Text::caption("One FrameRequest per dirty cycle, not per atom change"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(12.0)
                                .child(Text::heading("Atom-driven redraw demo"))
                                .child(Text::caption("Each button press triggers exactly one frame render"))
                                .child(
                                    Row::new().spacing(12.0)
                                        .child(Button::new("−").on_press(move || dec.set(cnt - 1)))
                                        .child(Text::display(&format!("{}", cnt)))
                                        .child(Button::new("+").on_press(move || inc.set(cnt + 1)))
                                )
                                .child(Text::caption(if cnt == 0 {
                                    "Idle at 0% CPU — no atom changed"
                                } else if cnt > 0 {
                                    "Frame was rendered — now sleeping again"
                                } else {
                                    "Negative! Still VSync — still 0% idle"
                                }))
                        ))
                )
            }

            Panel::LayoutCtx => {
                Box::new(
                    Column::new().spacing(16.0)
                        .child(nav)
                        .child(Text::display("LayoutCtx"))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("D056 — Layout has full context"))
                                .child(Text::caption("Before: fn layout(&self, constraints: Constraints) -> Size"))
                                .child(Text::caption("After:  fn layout(&self, ctx: &LayoutCtx) -> Size"))
                                .child(Text::caption("LayoutCtx carries: constraints + font + theme"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("ctx.with_constraints()"))
                                .child(Text::caption("Container widgets derive child context:"))
                                .child(Text::caption("  child.layout(&ctx.with_constraints(inner_c))"))
                                .child(Text::caption("Font and theme propagate automatically — no re-lookup"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("Size cache — Column + Row"))
                                .child(Text::caption("measure() result cached by (Constraints → Vec<Size>)"))
                                .child(Text::caption("paint() reuses the same sizes — zero second layout pass"))
                                .child(Text::caption("Mutex<Option<_>> provides Send+Sync interior mutability"))
                        ))
                )
            }

            Panel::TextMetrics => {
                Box::new(
                    Column::new().spacing(16.0)
                        .child(nav)
                        .child(Text::display("Accurate Text Measurement"))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("Before — character heuristic"))
                                .child(Text::caption("let est_w = text.len() as f32 * size * 0.6;"))
                                .child(Text::caption("height = size * 1.3"))
                                .child(Text::caption("Off by up to 40% for short strings, emoji, CJK"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("After — real glyph metrics"))
                                .child(Text::caption("let w = ctx.font.measure_text(&text, size);"))
                                .child(Text::caption("let h = ctx.font.line_height(size);"))
                                .child(Text::caption("Pixel-accurate — uses actual font advance widths"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(12.0)
                                .child(Text::heading("Width comparison"))
                                .child(Text::label("Short"))
                                .child(Text::label("A much longer string to see how centering improves"))
                                .child(Text::new("i").size(32.0).align(TextAlign::Center))
                                .child(Text::new("W").size(32.0).align(TextAlign::Center))
                                .child(Text::caption("'i' and 'W' now have correct distinct widths"))
                        ))
                )
            }

            Panel::Animation => {
                let spring_val: Atom<f32> = ctx.state(50.0_f32);
                let (animated, spring_ctrl) = use_spring(ctx, spring_val.get());
                let val = animated.get();

                let (anim_progress, anim_ctrl) = use_animation(ctx, Duration::from_millis(1500));
                let anim_val = anim_progress.get();
                let play_ctrl: AnimCtrl = anim_ctrl.clone();
                let pause_ctrl: AnimCtrl = anim_ctrl.clone();
                let reset_ctrl: AnimCtrl = anim_ctrl.clone();

                let lo_ctrl: SpringController = spring_ctrl.clone();
                let hi_ctrl: SpringController = spring_ctrl.clone();

                Box::new(
                    Column::new().spacing(16.0)
                        .child(nav)
                        .child(Text::display("Animation VSync Clock"))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("D059 — Real wall-clock dt"))
                                .child(Text::caption("Platform writes dt = now - last_frame before each render"))
                                .child(Text::caption("tezzera_animate::set_frame_dt(dt) → FRAME_DT atomic"))
                                .child(Text::caption("use_spring and use_animation read frame_dt() — never 1/60 hardcoded"))
                                .child(Text::caption("Frame-rate independent: same speed at 60Hz and 120Hz"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(12.0)
                                .child(Text::heading("use_animation — 1.5s progress bar"))
                                .child(Text::caption(format!("Progress: {:.0}%", anim_val * 100.0)))
                                .child(ProgressBar::new(anim_val))
                                .child(
                                    Row::new().spacing(8.0)
                                        .child(Button::new("Play").on_press(move || play_ctrl.play()))
                                        .child(Button::new("Pause").on_press(move || pause_ctrl.pause()))
                                        .child(Button::new("Reset").on_press(move || reset_ctrl.reset()))
                                )
                        ))
                        .child(Card::new(
                            Column::new().spacing(12.0)
                                .child(Text::heading("use_spring — physics-based bar"))
                                .child(Text::caption(format!("Current value: {:.1}", val)))
                                .child(ProgressBar::new(val / 100.0))
                                .child(
                                    Row::new().spacing(12.0)
                                        .child(Button::new("Spring to 0").on_press(
                                            move || lo_ctrl.animate_to(0.0)
                                        ))
                                        .child(Button::new("Spring to 100").on_press(
                                            move || hi_ctrl.animate_to(100.0)
                                        ))
                                )
                                .child(Text::caption("Press a button — the bar springs smoothly"))
                        ))
                )
            }

            Panel::Overlay => {
                let dropdown_open: Atom<bool> = ctx.state(false);
                let open = dropdown_open.clone();
                let close = dropdown_open.clone();
                let is_open = dropdown_open.get();

                let anchor: Atom<Option<Rect>> = ctx.state(None);

                if is_open {
                    let close_fn = close.clone();
                    let pos = anchor.get()
                        .map(|r| Point { x: r.origin.x, y: r.origin.y + r.size.height })
                        .unwrap_or(Point { x: 200.0, y: 300.0 });

                    push_overlay(
                        OverlayEntry::new(
                            LayerPosition::Absolute(pos),
                            Card::new(
                                Column::new().spacing(4.0)
                                    .child(Button::new("Option A").on_press({
                                        let c = close_fn.clone();
                                        move || c.set(false)
                                    }))
                                    .child(Button::new("Option B").on_press({
                                        let c = close_fn.clone();
                                        move || c.set(false)
                                    }))
                                    .child(Button::new("Option C").on_press({
                                        move || close_fn.set(false)
                                    }))
                            )
                        )
                        .input(InputBehavior::PassThrough)
                    );
                }

                Box::new(
                    Column::new().spacing(16.0)
                        .child(nav)
                        .child(Text::display("Overlay Layer"))
                        .child(Card::new(
                            Column::new().spacing(8.0)
                                .child(Text::heading("D057 — RectReader"))
                                .child(Text::caption("Fires atom.set(Some(ctx.rect)) after each paint"))
                                .child(Text::caption("Surfaces window-pixel coordinates to user code"))
                                .child(Text::caption("Composable — wraps any widget, no widget changes"))
                        ))
                        .child(Card::new(
                            Column::new().spacing(12.0)
                                .child(Text::heading("D058 — Overlay layer"))
                                .child(Text::caption("Second PictureRecorder pass, always on top of main tree"))
                                .child(Text::caption("push_overlay() from any widget's paint() call"))
                                .child(Text::caption(if is_open {
                                    "Dropdown is open — click an option to close"
                                } else {
                                    "Click the button below to open a dropdown overlay"
                                }))
                                .child(
                                    RectReader::new(
                                        anchor.clone(),
                                        Button::new(if is_open { "Close" } else { "Open Dropdown" })
                                            .on_press(move || open.set(!is_open))
                                    )
                                )
                                .child(Text::caption(
                                    match anchor.get() {
                                        Some(r) => format!(
                                            "Button at ({:.0},{:.0}) size ({:.0}×{:.0})",
                                            r.origin.x, r.origin.y, r.size.width, r.size.height
                                        ),
                                        None => "Button rect not yet captured".to_string(),
                                    }.as_str()
                                ))
                        ))
                )
            }
        };

        body.into_element()
    }
}

fn main() {
    App::new()
        .title("TEZZERA Phase 12 — VSync + LayoutCtx + Overlay")
        .size(900, 720)
        .launch(Phase12Demo);
}
