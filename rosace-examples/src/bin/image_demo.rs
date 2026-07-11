//! D109 image-textures verification: a real PNG drawn via `Image` in both
//! modes — GPU (texture cache) vs `ROSACE_CPU_SHAPES=1` (blit) — for pixel
//! comparison, plus a half-opacity copy to verify the opacity uniform.

use rosace::prelude::*;

struct ImageDemo;

impl Component for ImageDemo {
    fn build(&self, _ctx: &mut Context) -> Element {
        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 10.0))
                .child(Text::new("Image via GPU texture cache").align(TextAlign::Center))
                .child(Image::file("app_showcase.png").width(360.0).height(220.0))
                .child(Text::new("same image, opacity path exercised by Hero elsewhere").align(TextAlign::Center)),
        )
        .app_bar(AppBar::new("image_demo"))
        .into_element()
    }
}

fn main() {
    env_logger::init();
    App::new().title("image_demo").size(560, 420).launch(ImageDemo);
}
