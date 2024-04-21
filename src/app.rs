use std::env;
use std::fs;
use std::io;

use env_logger::Env;

use crate::renderloop;

pub struct MiniLessApp {
    // debug_log_file: fs::File,
}

impl MiniLessApp {
    pub fn new(log_filename: &str) -> Self {
        let rust_log_value = env::var("RUST_LOG").unwrap_or("".to_string());
        if rust_log_value == "debug" {
            let log_file = fs::File::create(log_filename).expect("Unable to create log file");
            env_logger::Builder::from_env(Env::default().default_filter_or("debug"))
                .target(env_logger::Target::Pipe(Box::new(log_file)))
                .init();
        }
        MiniLessApp {
            // debug_log_file: log_file,
        }
    }

    pub fn run(self, filename: &str) -> io::Result<()> {
        renderloop::less_loop(filename)
    }
}
