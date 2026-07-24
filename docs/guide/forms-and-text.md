# Forms & Text Input

Text fields are real editable text in ROSACE: click to focus, type, arrow-key navigation, Shift+arrow selection, Home/End, Cmd/Ctrl+A select-all, and clipboard shortcuts all work out of the box. This chapter covers the two text-editing widgets and the `rosace-forms` crate that binds them to validation.

## `TextInput`: single-line text

Like every other interactive widget in ROSACE, `TextInput` is **controlled** — you own the true value (typically a `ctx.state` atom), pass it in via `.value()`, and get edits back via `.on_change()`. It never mutates your state itself.

```rust
use rosace::prelude::*;

impl Component for LoginForm {
    fn build(&self, ctx: &mut Context) -> Element {
        let email = ctx.state(String::new());

        Column::new()
            .spacing(12.0)
            .padding(EdgeInsets::all(24.0))
            .child(
                TextInput::new()
                    .placeholder("Email")
                    .value(email.get())
                    .on_change({
                        let email = email.clone();
                        move |v| email.set(v)
                    }),
            )
            .into_element()
    }
}
```

Useful builder calls: `.placeholder(...)`, `.focused()` (seed focus on first paint), `.obscure()` (password field), `.width(...)`/`.height(...)`, `.background(...)`/`.border(...)`/`.focus_color(...)`, and `.keyboard_type(...)` (hints the soft keyboard layout on mobile — `Email`, `Numeric`, `Url`, `Phone`).

## `TextArea`: multi-line text

`TextArea` shares the same controlled-value shape as `TextInput`, plus scrolling for content taller than the box:

```rust
TextArea::new()
    .placeholder("Write something...")
    .value(body.get())
    .height(200.0)
    .on_change({
        let body = body.clone();
        move |v| body.set(v)
    })
```

It also takes `.no_scrollbar()` and `.scrollbar_color(...)` if you want to customize or hide the scrollbar.

## Syntax highlighting and custom carets

Both widgets accept `.spans(...)` — a closure that inspects the current text (and, after the first call, the byte range that just changed) and returns styled `Span`s, so you can build a markdown or code editor without the crate knowing anything about markdown or code:

```rust
TextArea::new()
    .value(source.get())
    .spans(|text, _changed_range| my_markdown_tokenizer(text))
```

`.cursor_style(CursorStyle { .. })` overrides the caret's width, color, corner radius, blink rate, and shape (`Bar`/`Block`/`Underline`/`Custom`) per field; unset, it falls back to a `CursorStyle` stashed on the theme via `ThemeData::with_ext` (see [Theming](theming.md)), then to a built-in default.

## Forms: `Form`, `FormField`, and validators

`rosace-forms` gives you a `FormField` — a named value with attached validation rules — and a `Form` that aggregates several fields:

```rust
use rosace::prelude::*;
use rosace_forms::{Form, FormField, Required, Email, MinLength};

impl Component for SignupForm {
    fn build(&self, ctx: &mut Context) -> Element {
        let email = FormField::for_ctx(ctx, "email").rule(Required).rule(Email);
        let name  = FormField::for_ctx(ctx, "name").rule(Required).rule(MinLength(2));
        let form  = Form::new().field(email.clone()).field(name.clone());

        Column::new()
            .spacing(12.0)
            .padding(EdgeInsets::all(24.0))
            .child(TextInput::new().placeholder("Name").field(name))
            .child(TextInput::new().placeholder("Email").field(email))
            .child(Button::new("Sign up").disabled_if(!form.is_valid()).on_press({
                let form = form.clone();
                move || { form.submit(|| { /* send it */ }); }
            }))
            .into_element()
    }
}
```

A few things to know about this shape:

- **`FormField::for_ctx(ctx, name)`** creates the field's value/errors/touched state as component state (same call-order rule as `ctx.state` — see [Components & State](components-and-state.md)) and subscribes the owning component to it, so writes to the field re-render this component. `FormField::new(name)` also exists for a throwaway field with no live UI updates.
- **`.rule(...)`** attaches a `Validator`. Built-in validators: `Required` (non-empty after trim), `MinLength(n)`, `MaxLength(n)`, `Contains("substr")`, `Email` (has `@` and a `.` after it — a simple check, not a full RFC parser), and `Range { min, max }` for numeric string fields.
- **`.field(field)`** on `TextInput`/`TextArea` is the binding: it seeds the widget's value from `field.get()`, installs an `on_change` that writes back and re-validates on every keystroke, and validates once immediately so an empty `Required` field is already reflected before the user types anything. Call `.on_change(...)` again *after* `.field(...)` if you need both — the later call wins.
- **`FormField` clones share state.** The `email` you pass to `TextInput::field(...)` and the `email` captured by `Form`/your submit closure are the same underlying atoms — no manual synchronization needed.
- **`form.is_valid()`** reflects the last validation run; because `.field(...)` validates on every change, it stays live as the user types. `form.submit(on_valid)` runs `validate_all()` across every field (touching all of them, so an untouched-but-invalid field's error becomes visible after a failed submit) and only calls `on_valid` if every field passed.

## Surfacing errors

`field.errors()` returns the current `Vec<FieldError>` (`{ field, message }`) after the last validation. Pair it with `is_touched()` so a blank required field doesn't show red before the user has had a chance to fill it in:

```rust
let mut col = Column::new()
    .child(TextInput::new().placeholder("Email").field(email.clone()));

if email.is_touched() && !email.is_valid() {
    col = col.child(
        Text::new(email.errors().first().map(|e| e.message.clone()).unwrap_or_default())
            .color(Color::rgb(213, 67, 61)), // or convert theme.colors.error — see Theming's color-type gotcha
    );
}
```

(`Form::errors()` collects every field's errors in one call if you want a single summary list instead of per-field captions.)

## What's not built yet

Mobile's native soft-keyboard IME (composition for CJK/emoji input on iOS/Android) and a text-selection magnifier loupe are both named, disclosed deferrals — desktop OS IME (composition, dead keys, CJK input) is real and wired today.

---

**Under the hood:** the layered text-editing core (keyboard dispatch, OS IME composition, selection/caret persistence on the render tree) is covered in [Widget Protocol](../architecture/widget-protocol.md).

Next: [Animation](animation.md).
