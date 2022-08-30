use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

pub struct SimpleSpinner;

impl SimpleSpinner {
    pub fn new_with_message(msg: Option<&str>) -> color_eyre::Result<ProgressBar> {
        let spinner = ProgressBar::new_spinner();
        spinner.enable_steady_tick(Duration::from_millis(400));
        spinner.set_style(
            ProgressStyle::with_template("{msg}{spinner}")?
            .tick_strings(&["🎸𝄢    ", "🎸𝄢𝅘𝅥𝅯   ", "🎸𝄢𝅘𝅥𝅯𝅘𝅥𝅮  ", "🎸𝄢𝅘𝅥𝅯𝅘𝅥𝅮𝅘𝅥 ", "🎸𝄢𝅘𝅥𝅯𝅘𝅥𝅮𝅘𝅥𝄽", "🎸𝄢    ", ]),
        );

        if let Some(msg) = msg {
            spinner.set_message(msg.to_owned());
        }

        Ok(spinner)
    }
}
