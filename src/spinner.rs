use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

const LEADER: char = ' ';
const DRUM: char = '🥁';
const QUARTER_NOTE: char = '♩';
const EIGHTH_NOTE: char = '♪';

pub struct SimpleSpinner;

impl SimpleSpinner {
    pub fn new_with_message(msg: Option<&str>) -> color_eyre::Result<ProgressBar> {
        let spinner = ProgressBar::new_spinner();
        spinner.enable_steady_tick(Duration::from_millis(260));
        spinner.set_style(
            ProgressStyle::with_template("{msg}{spinner}")?.tick_strings(&[
                // "Play" the quarter note for a whole 115bpm beat
                &([LEADER, DRUM, QUARTER_NOTE].into_iter().collect::<String>()),
                &([LEADER, DRUM, QUARTER_NOTE].into_iter().collect::<String>()),
                &([LEADER, DRUM, QUARTER_NOTE, EIGHTH_NOTE]
                    .into_iter()
                    .collect::<String>()),
                &([LEADER, DRUM, QUARTER_NOTE, EIGHTH_NOTE, EIGHTH_NOTE]
                    .into_iter()
                    .collect::<String>()),
                // indicatif appears to swallow this next frame, so ...
                &([LEADER, DRUM, QUARTER_NOTE, EIGHTH_NOTE, EIGHTH_NOTE]
                    .into_iter()
                    .collect::<String>()),
            ]),
        );

        if let Some(msg) = msg {
            spinner.set_message(msg.to_owned());
        }

        Ok(spinner)
    }
}
