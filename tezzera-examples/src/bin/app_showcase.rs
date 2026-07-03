//! Mosaic Task Manager — rebuilt as a real TEZZERA widget tree.
//!
//! This is not canvas drawing — every pixel comes from composable widgets:
//! Scaffold → NavRail + Column → Card → Row → Text / Badge / Checkbox / etc.
//!
//! Run:    cargo run -p tezzera-examples --bin app_showcase
//! Output: app_showcase.png (1400×900)

use tezzera::prelude::*;

// ── Palette ───────────────────────────────────────────────────────────────────
const BG:         Color = Color::rgb(15, 16, 28);
const SIDEBAR_BG: Color = Color::rgb(11, 12, 22);
#[allow(dead_code)]
const CARD_BG:    Color = Color::rgb(20, 22, 38);
const DETAIL_BG:  Color = Color::rgb(17, 19, 34);
const ITEM_HOV:   Color = Color::rgb(26, 29, 50);
const BORDER:     Color = Color::rgb(32, 35, 58);
const SEP:        Color = Color::rgb(24, 26, 44);

const ACCENT:     Color = Color::rgb(110,  75, 210);
const ACCENT_DIM: Color = Color::rgb( 60,  40, 120);
const GREEN:      Color = Color::rgb( 60, 195, 105);
const GREEN_DIM:  Color = Color::rgb( 24,  70,  40);
const ORANGE:     Color = Color::rgb(255, 155,  50);
#[allow(dead_code)]
const RED:        Color = Color::rgb(235,  75,  75);
const BLUE:       Color = Color::rgb( 70, 165, 255);
const BLUE_DIM:   Color = Color::rgb( 20,  55, 100);
const PINK:       Color = Color::rgb(210,  80, 170);
const PINK_DIM:   Color = Color::rgb( 70,  25,  56);

const TEXT:     Color = Color::rgb(220, 222, 240);
const TEXT_SUB: Color = Color::rgb(140, 144, 175);
const TEXT_DIM: Color = Color::rgb( 80,  85, 118);

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn sidebar() -> impl Widget {
    NavRail::new()
        .width(232.0)
        .background(SIDEBAR_BG)
        .widget(
            Container::new().padding(EdgeInsets::all(16.0)).child(Row::new()
                    .spacing(10.0)
                    .child(
                        Container::new()
                            .background(ACCENT)
                            .size(24.0, 24.0)
                            .radius(4.0)
                            .child(Container::new().align(Alignment::Center).child(Text::new("M").color(TEXT).size(13.0)))
                    )
                    .child(Text::new("Mosaic").color(TEXT).size(13.0))
            )
        )
        .separator()
        .section("OVERVIEW")
        .item(NavItem::new("Inbox").badge(5)
            .leading(Icon::new(IconKind::Inbox).size(14.0).color(TEXT_SUB)))
        .item(NavItem::new("Today").badge(7).active()
            .leading(Icon::new(IconKind::Calendar).size(14.0).color(ACCENT)))
        .item(NavItem::new("Upcoming").badge(12)
            .leading(Icon::new(IconKind::Star).size(14.0).color(TEXT_SUB)))
        .item(NavItem::new("Completed")
            .leading(Icon::new(IconKind::Check).size(14.0).color(TEXT_SUB)))
        .separator()
        .section("PROJECTS")
        .item(NavItem::new("TEZZERA Framework")
            .leading(Icon::new(IconKind::Dot).size(12.0).color(ACCENT)))
        .item(NavItem::new("Documentation Site")
            .leading(Icon::new(IconKind::Dot).size(12.0).color(BLUE)))
        .item(NavItem::new("Design System")
            .leading(Icon::new(IconKind::Dot).size(12.0).color(PINK)))
        .item(NavItem::new("API Integration")
            .leading(Icon::new(IconKind::Dot).size(12.0).color(GREEN)))
        .separator()
        .section("LABELS")
        .widget(
            Container::new().padding(EdgeInsets::only(4.0, 8.0, 4.0, 14.0)).child(Row::new().spacing(6.0)
                    .child(Chip::new("dev").selected())
                    .child(Chip::new("design"))
                    .child(Chip::new("planning"))
            )
        )
        .widget(
            Container::new().padding(EdgeInsets::only(4.0, 8.0, 8.0, 14.0)).child(Row::new().spacing(6.0)
                    .child(Chip::new("urgent").color(Color::rgb(50, 15, 15)).selected_color(Color::rgb(80, 24, 24)))
                    .child(Chip::new("writing"))
            )
        )
        // User profile pinned at bottom via Expanded spacer
        .widget(Expanded::empty())
        .separator()
        .widget(
            Container::new().padding(EdgeInsets::symmetric(16.0, 12.0)).child(Row::new().spacing(10.0)
                    .child(Avatar::new("G").size(32.0))
                    .child(
                        Column::new().spacing(2.0)
                            .child(Text::new("Godwin Joseph").color(TEXT).size(10.5))
                            .child(Text::new("godwin@tezzera.io").color(TEXT_DIM).size(8.5))
                    )
            )
        )
}

// ── Task row helper ───────────────────────────────────────────────────────────

fn task_row(title: &str, done: bool, tags: &[(&str, Color, Color)], selected: bool) -> impl Widget {
    let bg = if selected { ITEM_HOV } else { Color::rgba(0,0,0,0) };

    Column::new()
        .child(
            Container::new()
                .background(bg)
                .child(
                    Container::new().padding(EdgeInsets::symmetric(14.0, 10.0)).child(Row::new().spacing(10.0)
                            .cross_axis_alignment(tezzera::layout::CrossAxisAlignment::Center)
                            .child(Checkbox::new(done)
                                .color(if done { GREEN } else { ACCENT }))
                            .child(
                                Column::new().spacing(4.0)
                                    .child({
                                        let col = if done { TEXT_DIM } else { TEXT };
                                        Text::new(title).color(col).size(11.0)
                                    })
                                    .child(
                                        Row::new().spacing(6.0)
                                            .children(
                                                tags.iter().map(|(label, fg, bg)| -> tezzera::BoxedWidget {
                                                    Box::new(
                                                        Badge::label(*label)
                                                            .color(*bg)
                                                            .text_color(*fg)
                                                    )
                                                }).collect()
                                            )
                                    )
                            )
                    )
                )
        )
        .child(Divider::horizontal().color(SEP).indent(14.0))
}

// ── Main task list ─────────────────────────────────────────────────────────────

fn main_panel() -> impl Widget {
    Column::new()
        .child(
            Container::new().padding(EdgeInsets::symmetric(16.0, 12.0)).child(TextInput::new()
                    .placeholder("Search tasks...")
            )
        )
        .child(
            Container::new().padding(EdgeInsets::only(0.0, 16.0, 8.0, 16.0)).child(Row::new().spacing(6.0)
                    .child(Chip::new("All"))
                    .child(Chip::new("Today").selected())
                    .child(Chip::new("High priority"))
                    .child(Chip::new("Assigned to me"))
            )
        )
        .child(Divider::horizontal().color(BORDER))
        .child(
            ScrollView::fixed(
                Column::new()
                    .child(
                        Container::new().padding(EdgeInsets::only(8.0, 16.0, 0.0, 14.0)).child(Row::new().spacing(8.0)
                                .child(Text::new("Today").color(TEXT_SUB).size(9.0))
                                .child(Badge::count(4).color(BORDER).text_color(TEXT_DIM))
                        )
                    )
                    .child(task_row("Review TEZZERA Phase 11 exit criteria", true,
                        &[("dev", BLUE, BLUE_DIM), ("framework", GREEN, GREEN_DIM)], false))
                    .child(task_row("Fix text rendering alignment in canvas.rs", true,
                        &[("dev", BLUE, BLUE_DIM)], false))
                    .child(task_row("Write Mosaic showcase demo", false,
                        &[("dev", BLUE, BLUE_DIM), ("design", PINK, PINK_DIM)], true))
                    .child(task_row("Update steering docs to COMPLETE", false,
                        &[("planning", ACCENT, ACCENT_DIM)], false))
                    .child(Spacer::gap(0.0, 12.0))
                    .child(
                        Container::new().padding(EdgeInsets::only(8.0, 16.0, 0.0, 14.0)).child(Row::new().spacing(8.0)
                                .child(Text::new("Upcoming").color(TEXT_SUB).size(9.0))
                                .child(Badge::count(5).color(BORDER).text_color(TEXT_DIM))
                        )
                    )
                    .child(task_row("TEZZERA Phase 12 planning", false,
                        &[("planning", ACCENT, ACCENT_DIM), ("framework", GREEN, GREEN_DIM)], false))
                    .child(task_row("Write framework blog post", false,
                        &[("writing", ORANGE, Color::rgb(80,48,16))], false))
                    .child(task_row("GPU renderer research", false,
                        &[("dev", BLUE, BLUE_DIM)], false))
                    .child(task_row("Set up CI pipeline", false,
                        &[("dev", BLUE, BLUE_DIM)], false))
                    .child(task_row("Design TEZZERA website", false,
                        &[("design", PINK, PINK_DIM)], false))
            )
        )
}

// ── Detail panel ─────────────────────────────────────────────────────────────

fn detail_panel() -> impl Widget {
    Container::new()
        .background(DETAIL_BG)
        .child(
            ScrollView::fixed(
                Container::new().padding(EdgeInsets::all(20.0)).child(Column::new().spacing(0.0)
                        // Title
                        .child(Text::new("Write Mosaic showcase demo").color(TEXT).size(14.0))
                        .child(Spacer::gap(0.0, 6.0))
                        // Breadcrumb
                        .child(
                            Row::new().spacing(6.0)
                                .child(Text::new("TEZZERA Framework").color(TEXT_DIM).size(9.0))
                                .child(Text::new("/").color(TEXT_DIM).size(9.0))
                                .child(Text::new("Today").color(ACCENT).size(9.0))
                        )
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Status badges
                        .child(
                            Row::new().spacing(8.0)
                                .child(Badge::label("In Progress").color(Color::rgb(80,48,16)).text_color(ORANGE))
                                .child(Badge::label("Medium").color(BLUE_DIM).text_color(BLUE))
                        )
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Metadata
                        .child(meta_row("Assigned to", "Godwin Joseph", TEXT))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(meta_row("Due date", "Jun 30, 2026", ORANGE))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(meta_row("Project", "TEZZERA", ACCENT))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(meta_row("Created", "Jun 28, 2026", TEXT_DIM))
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Description
                        .child(Text::new("Description").color(TEXT_DIM).size(9.0))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Text::new("Build a polished Mosaic app screenshot").color(TEXT_SUB).size(9.5))
                        .child(Spacer::gap(0.0, 3.0))
                        .child(Text::new("using TEZZERA's real widget tree pipeline.").color(TEXT_SUB).size(9.5))
                        .child(Spacer::gap(0.0, 3.0))
                        .child(Text::new("Scaffold, NavRail, Column, Row, Card, Text,").color(TEXT_SUB).size(9.5))
                        .child(Spacer::gap(0.0, 3.0))
                        .child(Text::new("Checkbox, Badge, Chip — all composable.").color(TEXT_SUB).size(9.5))
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Subtasks
                        .child(Text::new("Subtasks").color(TEXT_DIM).size(9.0))
                        .child(Spacer::gap(0.0, 10.0))
                        .child(subtask("Design layout and color palette", true))
                        .child(subtask("Implement window chrome", true))
                        .child(subtask("Draw sidebar navigation", true))
                        .child(subtask("Render task list with groups", false))
                        .child(subtask("Build detail panel", false))
                        .child(subtask("Export as PNG", false))
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Progress
                        .child(
                            Row::new()
                                .main_axis_alignment(tezzera::layout::MainAxisAlignment::SpaceBetween)
                                .child(Text::new("Progress").color(TEXT_DIM).size(9.0))
                                .child(Text::new("3 / 6 done").color(TEXT_SUB).size(9.0))
                        )
                        .child(Spacer::gap(0.0, 8.0))
                        .child(ProgressBar::new(0.5).color(GREEN))
                        .child(Spacer::gap(0.0, 14.0))
                        .child(Divider::horizontal().color(BORDER))
                        .child(Spacer::gap(0.0, 12.0))
                        // Controls demo
                        .child(Text::new("Notifications").color(TEXT_DIM).size(9.0))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(
                            Row::new().spacing(10.0)
                                .child(Text::new("Email alerts").color(TEXT_SUB).size(9.5))
                                .child(Expanded::empty())
                                .child(Switch::new(true))
                        )
                        .child(Spacer::gap(0.0, 6.0))
                        .child(
                            Row::new().spacing(10.0)
                                .child(Text::new("Push notifications").color(TEXT_SUB).size(9.5))
                                .child(Expanded::empty())
                                .child(Switch::new(false))
                        )
                        .child(Spacer::gap(0.0, 14.0))
                        // Priority slider
                        .child(Text::new("Priority level").color(TEXT_DIM).size(9.0))
                        .child(Spacer::gap(0.0, 8.0))
                        .child(Slider::new(0.6).width(320.0))
                )
            )
        )
}

fn meta_row(label: &str, value: &str, val_color: Color) -> impl Widget {
    Row::new().spacing(0.0)
        .child(Container::new().width(90.0).child(Text::new(label).color(TEXT_DIM).size(9.0)))
        .child(Text::new(value).color(val_color).size(9.0))
}

fn subtask(label: &str, done: bool) -> impl Widget {
    Container::new().padding(EdgeInsets::only(0.0, 0.0, 6.0, 0.0)).child(Row::new().spacing(8.0)
            .child(Checkbox::new(done)
                .color(if done { GREEN } else { BORDER })
                .size(14.0))
            .child(Text::new(label)
                .color(if done { TEXT_DIM } else { TEXT_SUB })
                .size(9.5))
    )
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let app = WidgetApp::new(1400, 900);  // dark theme + background auto from theme

    let root = Scaffold::new(
        Row::new()
            .child(Expanded::new(main_panel()))
            .child(Divider::vertical().color(BORDER))
            .child(Container::new().width(380.0).child(detail_panel()))
    )
    .app_bar(
        AppBar::new("Mosaic")
            .background(Color::rgb(9, 10, 18))
            .height(44.0)
    )
    .nav_rail(sidebar())
    .background(BG);

    let png = app.render_png(&root);
    std::fs::write("app_showcase.png", &png).expect("write failed");
    println!("Saved app_showcase.png (1400×900) — built with TEZZERA widget tree");
}
