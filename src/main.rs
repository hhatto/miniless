use std::fs::File;
use std::io::stdout;
use std::io;

use clap::Parser;
use crossterm::{
    cursor::{
        position, DisableBlinking, MoveDown, MoveLeft, MoveRight, MoveTo, MoveUp, RestorePosition,
        SavePosition,
    },
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor},
    terminal,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, ScrollDown, ScrollUp},
};
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::SearcherBuilder;

#[derive(Parser)]
#[clap(
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
struct Opts {
    input: String,
}

#[derive(Debug)]
struct DisplayLines {
    start: u64,
    end: u64,
    cursor_pos: u64, // use with is_search_mode
}

impl DisplayLines {
    fn start_mut(&mut self) -> &mut u64 {
        &mut self.start
    }
    fn end_mut(&mut self) -> &mut u64 {
        &mut self.end
    }
    fn cursor_pos_mut(&mut self) -> &mut u64 {
        &mut self.cursor_pos
    }
}

#[derive(Clone, Debug)]
struct SearchResult {
    word: String,
    lines: Vec<u64>,
    now_idx: Option<usize>,
}

impl SearchResult {
    fn word_mut(&mut self) -> &mut String {
        &mut self.word
    }
    fn lines_mut(&mut self) -> &mut Vec<u64> {
        &mut self.lines
    }
    fn get_near_line(&mut self, now_position: u64) -> Option<u64> {
        let mut pos = None;
        for idx in 0..self.lines.clone().len() {
            if now_position >= self.lines[idx] {
                pos = Some(self.lines[idx]);
                self.now_idx = Some(idx);
                break;
            }
        }
        pos
    }
    fn next(&mut self) -> Option<u64> {
        let result_count = self.lines.len();
        match self.now_idx {
            Some(n) => {
                if result_count > 0 {
                    let update_n = if result_count > (n + 1) { n + 1 } else { 0 };
                    self.now_idx = Some(update_n);
                    Some(self.lines[update_n])
                } else {
                    None
                }
            }
            None => None,
        }
    }
}

const STATUS_LINE_OFFSET: usize = 2;
const DISPLAY_BOTTOM_LINE_OFFSET: usize = STATUS_LINE_OFFSET + 1;

fn search(filename: &str, search_word: &str) -> io::Result<Vec<u64>> {
    let matcher = RegexMatcher::new(search_word).unwrap();
    let mut matches: Vec<u64> = vec![];
    let mut searcher = SearcherBuilder::new().build();
    searcher.search_path(
        &matcher,
        filename,
        UTF8(|lnum, _line| {
            matches.push(lnum);
            Ok(true)
        }),
    )?;
    Ok(matches)
}

fn clear_status_line() -> io::Result<()> {
    let (window_columns, window_rows) = terminal::size()?;
    let status_line = vec![" ";window_columns as usize];

    execute!(
        stdout(),
        SavePosition,
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16),
        Print(String::from_iter(status_line)),
        RestorePosition,
    )?;

    Ok(())
}

fn render_status_line(
    line_count: u64,
    max_line_count: usize,
    display_lines: &DisplayLines,
    search_result: &SearchResult,
) -> io::Result<()> {
    let (window_columns, window_rows) = terminal::size()?;
    let status_line = vec![" ";window_columns as usize];

    let percentage = line_count as f64 / max_line_count as f64 * 100.;
    let l = format!(
        "{}/{}({:3.0}%) search={}, {:?}, {:?}, {:?}",
        line_count,
        max_line_count,
        percentage as usize,
        search_result.word,
        search_result.lines,
        search_result,
        display_lines
    );

    execute!(
        stdout(),
        SavePosition,
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16),
        SetBackgroundColor(Color::Blue),
        Print(String::from_iter(status_line)),
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16),
        Print(l),
        ResetColor,
        RestorePosition,
    )?;

    Ok(())
}

fn less_loop(filename: &str) -> io::Result<()> {
    let f = File::open(filename)?;
    let lines = ropey::Rope::from_reader(f)?;
    let line_count = lines.len_lines() - 1;
    let mut is_search_mode = false;

    let mut search_result = SearchResult {
        word: String::new(),
        lines: Vec::new(),
        now_idx: None,
    };
    let mut search_word_vec: Vec<char> = [].to_vec();
    let (_, window_rows) = terminal::size()?;
    let mut display_lines = DisplayLines {
        start: 0,
        end: 0,
        cursor_pos: 0,
    };

    for idx in 0..(window_rows - STATUS_LINE_OFFSET as u16) {
        *display_lines.end_mut() = idx as u64;
        // NOTE: use format, because debug print
        let disp = format!("{}", lines.line(idx as usize));
        execute!(stdout(), MoveTo(0, idx), Print(disp))?;
        if idx >= line_count as u16 {
            break;
        }
    }
    execute!(stdout(), MoveTo(0, 0), SavePosition)?;

    loop {
        let (_, row) = position()?;
        let _ = render_status_line(
            display_lines.start + 1 + row as u64,
            line_count,
            &display_lines,
            &search_result,
        );

        let event = read()?;

        let _ = clear_status_line();

        if is_search_mode {
            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Esc, ..
                }) => {
                    is_search_mode = false;
                    execute!(stdout(), RestorePosition)?;
                    search_word_vec = Vec::new();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Enter, ..
                }) => {
                    // set search word
                    if search_word_vec.is_empty() {
                        continue;
                    }
                    *search_result.word_mut() = String::from_iter(search_word_vec.clone());

                    // get search result
                    let result = search(filename, search_result.word.as_str())?;
                    if !result.is_empty() {
                        // set search result
                        *search_result.lines_mut() = result;

                        let now_position = display_lines.start + row as u64;
                        if let Some(lnum) = search_result.get_near_line(now_position) {
                            // jump to result line
                            execute!(stdout(), RestorePosition, Clear(ClearType::All))?;

                            for idx in 0..(window_rows - STATUS_LINE_OFFSET as u16) {
                                let l = lines.line(lnum as usize + idx as usize - 1);
                                execute!(stdout(), MoveTo(0, idx), Print(format!("{}", l)))?;
                                if idx as usize >= line_count - 1 {
                                    break;
                                }
                            }
                            *display_lines.start_mut() += lnum - 1;
                            *display_lines.end_mut() += lnum - 1;
                            execute!(stdout(), MoveTo(0, 0))?;
                        }
                    }

                    is_search_mode = false;
                    execute!(stdout(), MoveTo(0, display_lines.cursor_pos as u16))?;
                    search_word_vec = Vec::new();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char(c), ..
                }) => {
                    search_word_vec.push(c);
                    execute!(stdout(), Print(c))?;
                }
                _ => (),
            };
        } else {
            execute!(stdout(), SavePosition)?;

            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Char('h') | KeyCode::Left, ..
                }) => execute!(stdout(), MoveLeft(1))?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('j') | KeyCode::Down, ..
                }) => {
                    if (window_rows - DISPLAY_BOTTOM_LINE_OFFSET as u16) == row
                        && line_count != (display_lines.end + 1) as usize
                    {
                        *display_lines.start_mut() = display_lines.start + 1;
                        *display_lines.end_mut() = display_lines.end + 1;
                        let l = lines.line(display_lines.end as usize);
                        // NOTE: use format because debug print
                        execute!(
                            stdout(),
                            ScrollUp(1),
                            Print(format!("{}", l)),
                            RestorePosition
                        )?;
                    } else if (window_rows - DISPLAY_BOTTOM_LINE_OFFSET as u16) != row
                        && line_count != (row + 1) as usize
                    {
                        execute!(stdout(), MoveDown(1))?;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('k') | KeyCode::Up, ..
                }) => {
                    if 0 == row && display_lines.start > 0 {
                        *display_lines.start_mut() = display_lines.start - 1;
                        *display_lines.end_mut() = display_lines.end - 1;
                        let l = lines.line(display_lines.start as usize);
                        // NOTE: use format because debug print
                        execute!(
                            stdout(),
                            ScrollDown(1),
                            Print(format!("{}", l)),
                            RestorePosition
                        )?;
                    } else if 0 != row {
                        execute!(stdout(), MoveUp(1))?;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('l') | KeyCode::Right, ..
                }) => execute!(stdout(), MoveRight(1))?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('u'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }) => execute!(stdout(), MoveUp(20))?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('d'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }) => execute!(stdout(), MoveDown(20))?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('/'), ..
                }) => {
                    is_search_mode = true;
                    *display_lines.cursor_pos_mut() = row as u64;
                    execute!(
                        stdout(),
                        SavePosition,
                        MoveTo(0, window_rows + 1),
                        Print("/"),
                    )?;
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('n'), ..
                }) => {
                    // jump next search result
                    // let now_position = display_lines.start + row as u64;
                    if search_result.now_idx.is_some() {
                        if let Some(lnum) = search_result.next() {
                            // jump to result line
                            execute!(
                                stdout(),
                                RestorePosition,
                                SavePosition,
                                Clear(ClearType::All)
                            )?;

                            for idx in 0..(window_rows - STATUS_LINE_OFFSET as u16) {
                                let l = lines.line(lnum as usize + idx as usize - 1);
                                execute!(stdout(), MoveTo(0, idx), Print(l))?;
                                if idx as usize >= line_count - 1 {
                                    break;
                                }
                            }
                            *display_lines.start_mut() += lnum - 1;
                            *display_lines.end_mut() += lnum - 1;
                            execute!(stdout(), RestorePosition)?;
                        };
                    };
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Esc, ..
                }) => break,
                _ => (),
            };
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let opts: Opts = Opts::parse();
    let mut stdout = stdout();

    enable_raw_mode()?;

    execute!(stdout, Clear(ClearType::All))?;

    execute!(stdout, MoveTo(0, 0), DisableBlinking,)?;

    if let Err(e) = less_loop(opts.input.as_str()) {
        println!("error={:?}\r", e);
    }

    disable_raw_mode()
}
