use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::time::Duration;

pub(crate) fn configure(progress: &ProgressBar, template: &str) {
    progress.set_style(
        ProgressStyle::with_template(template)
            .expect("valid progress template")
            // Indicatif reserves the last frame for completion, outside the animation loop.
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", " "])
            .progress_chars("=> "),
    );
    progress.enable_steady_tick(Duration::from_millis(200));
}

pub(crate) fn progress_spinner(message: &'static str, visible: bool) -> ProgressBar {
    let progress = ProgressBar::with_draw_target(
        None,
        if visible {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        },
    );
    configure(&progress, "{spinner:.green} [{elapsed_precise}] {msg}");
    progress.set_message(message);
    progress
}
