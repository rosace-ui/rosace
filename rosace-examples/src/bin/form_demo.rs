//! D116/Phase 28 Step 8 verification: `rosace-forms` wired into
//! `TextInput` for real — `.field(FormField)` binding, live inline
//! validation errors, a submit button gated on `Form::is_valid()`, and
//! an input filter (digits-only) on the phone field.

use rosace::prelude::*;

struct FormDemo;

impl Component for FormDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let name = FormField::for_ctx(ctx, "name").rule(Required).rule(MinLength(2));
        let email = FormField::for_ctx(ctx, "email").rule(Required).rule(Email);
        let phone = FormField::for_ctx(ctx, "phone").rule(MinLength(7));
        let submitted: Atom<bool> = ctx.state(false);

        let form = Form::new()
            .field(name.clone())
            .field(email.clone())
            .field(phone.clone());

        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 20.0))
                .child(Text::new("Sign up").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 16.0))
                .child(TextInput::new().placeholder("Name").width(280.0).field(name))
                .child(Spacer::gap(0.0, 10.0))
                .child(TextInput::new().placeholder("Email").width(280.0).field(email))
                .child(Spacer::gap(0.0, 10.0))
                .child(
                    TextInput::new()
                        .placeholder("Phone (digits only)")
                        .width(280.0)
                        .filters(vec![InputFilter::digits()])
                        .field(phone),
                )
                .child(Spacer::gap(0.0, 16.0))
                .child(Button::new("Submit").disabled_if(!form.is_valid()).on_press({
                    let submitted = submitted.clone();
                    move || {
                        let submitted = submitted.clone();
                        form.submit(move || submitted.set(true));
                    }
                }))
                .child(Spacer::gap(0.0, 10.0))
                .child(Text::new(if submitted.get() { "Submitted!" } else { "" }).align(TextAlign::Center)),
        )
        .app_bar(AppBar::new("form_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    App::new().title("form_demo").size(400, 480).launch(FormDemo);
}
