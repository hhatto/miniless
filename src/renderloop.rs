use std::fs::File;
use std::io;
use std::io::stdout;

use crossterm::{
    cursor::{position, MoveDown, MoveLeft, MoveRight, MoveTo, MoveUp, RestorePosition, SavePosition},
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor},
    terminal,
    terminal::{Clear, ClearType, ScrollDown, ScrollUp},
};

use crate::search;
use crate::search::SearchResult;
use crate::utils;

const DEBUG: bool = true;
const STATUS_LINE_OFFSET: usize = 2;
const DISPLAY_BOTTOM_LINE_OFFSET: usize = STATUS_LINE_OFFSET + 1;

#[derive(Debug)]
struct DisplayLines {
    start: u64,
    end: u64,
    cursor_pos: (u64, u64), // use with is_search_word_input_mode, (row, col)
}

impl DisplayLines {
    fn start_mut(&mut self) -> &mut u64 {
        &mut self.start
    }
    fn end_mut(&mut self) -> &mut u64 {
        &mut self.end
    }
    fn cursor_pos_mut(&mut self) -> &mut (u64, u64) {
        &mut self.cursor_pos
    }
}

fn clear_status_line() -> io::Result<()> {
    let (window_columns, window_rows) = terminal::size()?;
    let status_line = vec![" "; window_columns as usize];

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
    col_num: u64,
    display_lines: &DisplayLines,
    search_result: &SearchResult,
) -> io::Result<()> {
    let (window_columns, window_rows) = terminal::size()?;
    let status_line = vec![" "; window_columns as usize];

    let percentage = line_count as f64 / max_line_count as f64 * 100.;
    let l = if DEBUG {
        format!(
            "{}/{}({:3.0}%) search={:?}, {:?}, {:?}",
            line_count,
            max_line_count,
            percentage as usize,
            search_result.word,
            search_result.lines.len(),
            display_lines
        )
    } else {
        format!("{}/{}({:3.0}%)", line_count, max_line_count, percentage as usize,)
    };

    let right_pane_string = format!("{}:{}", line_count, col_num);

    execute!(
        stdout(),
        SavePosition,
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16),
        SetBackgroundColor(Color::Blue),
        Print(String::from_iter(status_line)),
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16),
        Print(l),
        MoveTo(window_columns - right_pane_string.len() as u16, window_rows - STATUS_LINE_OFFSET as u16),
        Print(right_pane_string),
        ResetColor,
        RestorePosition,
    )?;

    Ok(())
}

fn clear_search_line() -> io::Result<()> {
    let (window_columns, window_rows) = terminal::size()?;
    let empty_line = vec![" "; window_columns as usize];

    execute!(
        stdout(),
        SavePosition,
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16 + 1),
        Print(String::from_iter(empty_line)),
        RestorePosition,
    )?;

    Ok(())
}

fn render_search_line(search_result: &SearchResult) -> io::Result<()> {
    let (_, window_rows) = terminal::size()?;
    let render_string = if search_result.word.is_empty() {
        String::from("")
    } else {
        format!("/{}", search_result.word)
    };
    execute!(
        stdout(),
        SavePosition,
        MoveTo(0, window_rows - STATUS_LINE_OFFSET as u16 + 1),
        Print(render_string),
        RestorePosition,
    )?;

    Ok(())
}

fn re_render_display_lines(lines: &ropey::Rope, start_line_num: usize, window_rows: u16) -> io::Result<()> {
    let line_count = lines.len_lines() - 1;

    for idx in 0..(window_rows - STATUS_LINE_OFFSET as u16) {
        let l = lines.line(start_line_num + idx as usize - 1);
        execute!(stdout(), MoveTo(0, idx), Print(l))?;
        if idx as usize >= line_count - 1 {
            break;
        }
    }

    Ok(())
}

fn handler_search_word_input_mode(
    display_lines: &mut DisplayLines,
    window_rows: u16,
    lines: &ropey::Rope,
    event: &Event,
    is_search_word_input_mode: bool,
    search_result: &mut SearchResult,
) -> io::Result<bool> {
    let mut return_search_word_input_mode = is_search_word_input_mode;
    match event {
        Event::Key(KeyEvent { code: KeyCode::Esc, .. }) => {
            return_search_word_input_mode = false;
            execute!(stdout(), RestorePosition)?;
            *search_result.word_vec_mut() = Vec::new();
        }
        Event::Key(KeyEvent {
            code: KeyCode::Delete | KeyCode::Backspace,
            ..
        }) => {
            if !search_result.word_vec.is_empty() {
                search_result.word_vec.pop();
                execute!(stdout(), MoveLeft(1), terminal::Clear(ClearType::FromCursorDown))?;
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Enter, ..
        }) => {
            let word_vec = search_result.word_vec.clone();
            search_result.reset();

            // set search word
            let mut lcol = display_lines.cursor_pos.1;
            if !word_vec.is_empty() {
                *search_result.word_mut() = String::from_iter(word_vec.clone());

                // get search result
                let result = search::search(search_result.filename, search_result.word.as_str())?;
                if !result.is_empty() {
                    // set search result
                    *search_result.lines_mut() = result;

                    let now_position = display_lines.start + display_lines.cursor_pos.0;
                    if let Some((lnum, _lcol)) = search_result.get_near_line(now_position) {
                        lcol = _lcol;
                        // jump to result line
                        execute!(stdout(), RestorePosition, SavePosition, Clear(ClearType::All))?;

                        re_render_display_lines(lines, lnum as usize, window_rows)?;

                        *display_lines.start_mut() = lnum - 1;
                        *display_lines.end_mut() = lnum - 1;
                        execute!(stdout(), MoveTo(0, 0))?;
                    }
                }
            } else {
                clear_search_line()?;
            }

            return_search_word_input_mode = false;
            execute!(stdout(), MoveTo(0, 0))?;
            execute!(stdout(), MoveTo(lcol as u16, display_lines.cursor_pos.0 as u16))?;
            *search_result.word_vec_mut() = Vec::new();
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char(c), ..
        }) => {
            search_result.word_vec.push(*c);
            execute!(stdout(), Print(c))?;
        }
        _ => (),
    };
    Ok(return_search_word_input_mode)
}

pub fn less_loop(filename: &str) -> io::Result<()> {
    let f = File::open(filename)?;
    let lines = ropey::Rope::from_reader(f)?;
    let line_count = lines.len_lines() - 1;
    let mut is_search_word_input_mode = false;

    let mut search_result = SearchResult::new(filename);
    let (_, window_rows) = terminal::size()?;
    let mut display_lines = DisplayLines {
        start: 0,
        end: 0,
        cursor_pos: (0, 0),
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

    // let mut shadow_cursor_pos_col = 0;

    loop {
        let (cursor_pos_col, cursor_pos_row) = position()?;
        let now_line_num = display_lines.start + 1 + cursor_pos_row as u64;
        let now_line_idx = now_line_num as usize - 1;
        let now_line = lines.line(now_line_idx);
        let line_len = if let Some(v) = now_line.as_str() {
            v.trim_end().len()
        } else {
            0
        };

        let _ = render_status_line(now_line_num, line_count, cursor_pos_col as u64 + 1, &display_lines, &search_result);

        let event = read()?;

        let _ = clear_status_line();

        if is_search_word_input_mode {
            is_search_word_input_mode = handler_search_word_input_mode(
                &mut display_lines,
                window_rows,
                &lines,
                &event,
                is_search_word_input_mode,
                &mut search_result,
            )?;
        } else {
            let _ = render_search_line(&search_result);

            execute!(stdout(), SavePosition)?;

            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Char('h') | KeyCode::Left,
                    ..
                }) => execute!(stdout(), MoveLeft(1))?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('j') | KeyCode::Down,
                    ..
                }) => {
                    let mut col_diff = 0;

                    if (window_rows - DISPLAY_BOTTOM_LINE_OFFSET as u16) == cursor_pos_row
                        && line_count != (display_lines.end + 1) as usize
                    {
                        *display_lines.start_mut() = display_lines.start + 1;
                        *display_lines.end_mut() = display_lines.end + 1;
                        let l = lines.line(display_lines.end as usize);
                        // NOTE: use format because debug print
                        execute!(
                            stdout(),
                            ScrollUp(1),
                            SavePosition,
                            MoveLeft(cursor_pos_col),
                            Print(format!("{}", l)),
                            RestorePosition
                        )?;

                        // TODO: last line
                        let now_line = lines.line(now_line_idx + 1);
                        let mut line_len = utils::line::get_stripped_line_length(now_line);
                        line_len = line_len.saturating_sub(1);
                        if cursor_pos_col > line_len as u16 {
                            col_diff = cursor_pos_col - line_len as u16;
                        }
                    } else if (window_rows - DISPLAY_BOTTOM_LINE_OFFSET as u16) != cursor_pos_row
                        && line_count != (cursor_pos_row + 1) as usize
                    {
                        execute!(stdout(), MoveDown(1))?;

                        // reset cursor position when line length is shorter than cursor position
                        let now_line = lines.line(now_line_idx + 1);
                        let mut line_len = utils::line::get_stripped_line_length(now_line);
                        // if shadow_cursor_pos_col != 0 && shadow_cursor_pos_col < line_len as u16 {
                        //     execute!(stdout(), MoveRight(shadow_cursor_pos_col - cursor_pos_col))?;
                        // }
                        line_len = line_len.saturating_sub(1);
                        if cursor_pos_col > line_len as u16 {
                            col_diff = cursor_pos_col - line_len as u16;
                        }
                    }

                    if col_diff > 0 {
                        // shadow_cursor_pos_col = cursor_pos_col;
                        execute!(stdout(), MoveLeft(col_diff))?;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('k') | KeyCode::Up,
                    ..
                }) => {
                    let mut col_diff = 0;
                    if 0 == cursor_pos_row && display_lines.start > 0 {
                        *display_lines.start_mut() = display_lines.start - 1;
                        *display_lines.end_mut() = display_lines.end - 1;
                        let l = lines.line(display_lines.start as usize);
                        // NOTE: use format because debug print
                        execute!(
                            stdout(),
                            ScrollDown(1),
                            SavePosition,
                            MoveLeft(cursor_pos_col),
                            Print(format!("{}", l)),
                            RestorePosition
                        )?;

                        // TODO: first line
                        let now_line = lines.line(now_line_idx - 1);
                        let mut line_len = utils::line::get_stripped_line_length(now_line);
                        line_len = line_len.saturating_sub(1);
                        if cursor_pos_col > line_len as u16 {
                            col_diff = cursor_pos_col - line_len as u16;
                        }
                    } else if 0 != cursor_pos_row {
                        execute!(stdout(), MoveUp(1))?;

                        // reset cursor position when line length is shorter than cursor position
                        let now_line = lines.line(now_line_idx - 1);
                        let mut line_len = utils::line::get_stripped_line_length(now_line);
                        line_len = line_len.saturating_sub(1);
                        if cursor_pos_col > line_len as u16 {
                            col_diff = cursor_pos_col - line_len as u16;
                        }
                    }

                    if col_diff > 0 {
                        execute!(stdout(), MoveLeft(col_diff))?;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('l') | KeyCode::Right,
                    ..
                }) => {
                    if line_len as u16 - 1 > cursor_pos_col {
                        execute!(stdout(), MoveRight(1))?
                    }
                }
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
                    code: KeyCode::Char('/'),
                    ..
                }) => {
                    is_search_word_input_mode = true;
                    *display_lines.cursor_pos_mut() = (cursor_pos_row as u64, cursor_pos_col as u64);
                    clear_search_line()?;
                    execute!(stdout(), SavePosition, MoveTo(0, window_rows + 1), Print("/"))?;
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char('n'),
                    ..
                }) => {
                    // jump next search result
                    // let now_position = display_lines.start + cursor_pos_row as u64;
                    if search_result.clone().exists_match() {
                        if let Some((lnum, lcol)) = search_result.next() {
                            // jump to result line
                            execute!(stdout(), RestorePosition, SavePosition, Clear(ClearType::All))?;

                            re_render_display_lines(&lines, lnum as usize, window_rows)?;

                            *display_lines.start_mut() = lnum - 1;
                            *display_lines.end_mut() = lnum - 1;
                            execute!(stdout(), RestorePosition, MoveTo(0, 0), MoveTo(lcol as u16, 0))?;
                        };
                    };

                    // TODO: render search line
                }
                Event::Key(KeyEvent { code: KeyCode::Esc, .. }) => break,
                _ => (),
            };
        }
    }

    Ok(())
}
