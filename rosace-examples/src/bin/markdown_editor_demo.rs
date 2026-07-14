//! D116/Phase 28 capstone demo — a real, usable markdown editor built
//! ONLY from public `TextArea` APIs: a toy `**bold**`/`# heading`/
//! `` `code` `` `SpanSource` (Step 5) does live syntax highlighting, a
//! toolbar's Bold/Italic buttons wrap the live keyboard selection via
//! `EditController` (Step 2), and a second `TextArea` mirrors the same
//! atom with the same highlighting as a live "preview" pane.
//!
//! The point this proves: `rosace` never learned what markdown is. The
//! app supplied the tokenizer (`markdown_spans` below) and the toolbar;
//! the framework only supplied the editing/selection/styling seams
//! (`Transaction`/`Selection`/`EditController`/`SpanSource`) every other
//! text-shaped widget already uses.

use rosace::prelude::*;

/// A deliberately toy markdown tokenizer — real enough to prove the
/// `SpanSource` seam works, not a spec-complete markdown parser (that's
/// explicitly the app's job, never the framework's, per D116).
fn markdown_spans(s: &str, _changed: Option<(usize, usize)>) -> Vec<Span> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut spans = Vec::new();

    // Headings: any line starting with "# ".
    let mut line_start = 0usize;
    for i in 0..=n {
        if i == n || chars[i] == '\n' {
            if i > line_start + 1 && chars[line_start] == '#' && chars[line_start + 1] == ' ' {
                spans.push(Span::new((line_start, i))
                    .color(Color::rgb(140, 180, 255))
                    .weight(FontWeight::Bold));
            }
            line_start = i + 1;
        }
    }

    // Bold: **...**
    let mut i = 0;
    while i + 1 < n {
        if chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(rel) = chars[i + 2..].windows(2).position(|w| w == ['*', '*']) {
                let end = i + 2 + rel + 2;
                spans.push(Span::new((i, end)).weight(FontWeight::Bold));
                i = end;
                continue;
            }
        }
        i += 1;
    }

    // Inline code: `...`
    let mut i = 0;
    while i < n {
        if chars[i] == '`' {
            if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '`') {
                let end = i + 1 + rel + 1;
                spans.push(Span::new((i, end)).color(Color::rgb(255, 180, 120)));
                i = end;
                continue;
            }
        }
        i += 1;
    }

    spans
}

struct MarkdownEditorDemo;

impl Component for MarkdownEditorDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let body: Atom<String> = ctx.state(String::from(
            "# Markdown demo\n\nType **bold** text, `inline code`, or a heading.\n\nThe toolbar wraps your selection.\n\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10\nLine 11\nLine 12\nLine 13\nLine 14\nLine 15\nLine 16\nLine 17\nLine 18\nLine 19\nLine 20"
        ));
        let controller: EditController = ctx.state(EditController::new()).get();

        let toolbar = Row::new()
            .child(Button::new("Bold").on_press({
                let controller = controller.clone();
                move || {
                    let value = controller.value();
                    let (start, end) = controller.selection().primary_range();
                    if start < end {
                        let word = value[start..end].to_string();
                        controller.replace_range(start, end, format!("**{word}**"));
                    }
                }
            }))
            .child(Spacer::gap(8.0, 0.0))
            .child(Button::new("Italic").on_press({
                let controller = controller.clone();
                move || {
                    let value = controller.value();
                    let (start, end) = controller.selection().primary_range();
                    if start < end {
                        let word = value[start..end].to_string();
                        controller.replace_range(start, end, format!("*{word}*"));
                    }
                }
            }));

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 16.0))
                .child(toolbar)
                .child(Spacer::gap(0.0, 12.0))
                .child(
                    Row::new()
                        .child(
                            TextArea::new()
                                .placeholder("Write markdown...")
                                .value(body.get())
                                .width(280.0)
                                .height(260.0)
                                .controller(controller.clone())
                                .spans(markdown_spans)
                                .on_change({
                                    let body = body.clone();
                                    move |v| body.set(v)
                                }),
                        )
                        .child(Spacer::gap(16.0, 0.0))
                        // Live preview pane — same atom, same highlighting,
                        // no `on_change` wired (a controlled widget with no
                        // listener silently ignores edits, the same
                        // convention every other widget here uses).
                        .child(
                            TextArea::new()
                                .value(body.get())
                                .width(280.0)
                                .height(260.0)
                                .spans(markdown_spans),
                        ),
                ),
        )
        .app_bar(AppBar::new("markdown_editor_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    App::new().title("markdown_editor_demo").size(620, 420).launch(MarkdownEditorDemo);
}
