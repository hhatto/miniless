use log::debug;
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
const CURSOR_JUMP_OFFSET: u16 = 30;

#[derive(Debug)]
struct DisplayLines {
    start: u64,
    end: u64,
    // use with is_search_word_input_mode, (row, col)
    cursor_pos: (u64, u64),
    // (row, col). only use col, now
    shadow_cursor_pos: (u64, u64),
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
    fn shadow_cursor_pos_mut(&mut self) -> &mut (u64, u64) {
        &mut self.shadow_cursor_pos
    }
}

fn is_required_correction_cursor_col(col: u64, before_col: u64, line_len: u64) -> u16 {
    if col == before_col {
        return 0;
    }
    if line_len > before_col {
        return before_col as u16;
    }
    if line_len == 0 || line_len == 1 {
        return 0;
    }
    line_len as u16 - 1
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
        let (cursor_pos_col, cursor_pos_row) = position()?;
        format!(
            "{}/{}({:3.0}%) pos={:?}, search={:?}, {:?}, {:?}",
            line_count,
            max_line_count,
            percentage as usize,
            (cursor_pos_row, cursor_pos_col),
            search_result.word,
            search_result.match_lines.len(),
            // search_result.now_idx,
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
        let offset = start_line_num + idx as usize - 1;
        if offset >= line_count {
            break;
        }
        let l = lines.line(offset);
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
            // clear search line and restore cursor position in display area
            return_search_word_input_mode = false;
            clear_search_line()?;
            execute!(
                stdout(),
                MoveTo(0, 0),
                MoveTo(display_lines.cursor_pos.1 as u16, display_lines.cursor_pos.0 as u16)
            )?;
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
                    *search_result.match_lines_mut() = result;

                    let now_position_row = display_lines.start + display_lines.cursor_pos.0;
                    let now_position_col = display_lines.cursor_pos.1;
                    if let Some((lnum, _lcol)) = search_result.get_near_line((now_position_row, now_position_col)) {
                        lcol = _lcol;
                        // jump to result line
                        execute!(stdout(), RestorePosition, SavePosition, Clear(ClearType::All))?;

                        re_render_display_lines(lines, lnum as usize, window_rows)?;
                        render_search_line(search_result)?;

                        *display_lines.start_mut() = lnum - 1;
                        // TODO: check this
                        *display_lines.end_mut() = lnum - 1;
                        // *display_lines.end_mut() = lnum + window_rows as u64 - STATUS_LINE_OFFSET as u64 - 1;
                        execute!(stdout(), MoveTo(0, 0))?;
                    }
                }
            } else {
                clear_search_line()?;
            }

            return_search_word_input_mode = false;
            execute!(stdout(), MoveTo(0, 0))?;
            execute!(stdout(), MoveTo(lcol as u16, 0))?;
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

#[allow(clippy::too_many_arguments)]
fn handler_display_input_mode(
    display_lines: &mut DisplayLines,
    window_rows: u16,
    cursor_pos_row: u16,
    cursor_pos_col: u16,
    now_line_idx: usize,
    line_count: usize,
    lines: &ropey::Rope,
    event: &Event,
    is_search_word_input_mode: bool,
    search_result: &mut SearchResult,
) -> io::Result<bool> {
    let mut return_search_word_input_mode = is_search_word_input_mode;
    let now_line = lines.line(now_line_idx);
    let line_len = if let Some(v) = now_line.as_str() {
        v.trim_end().len()
    } else {
        0
    };

    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('j') | KeyCode::Down,
            ..
        }) => {
            let mut col_diff = 0;
            let mut next_line_len = 0;
            // save cursor position before move
            let before_cursor_pos_col = display_lines.shadow_cursor_pos.1;

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
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64 + 1, before_cursor_pos_col);

                // TODO: last line
                let now_line = lines.line(now_line_idx + 1);
                next_line_len = utils::line::get_stripped_line_length(now_line);
                next_line_len = next_line_len.saturating_sub(1);
                if cursor_pos_col > next_line_len as u16 {
                    col_diff = cursor_pos_col - next_line_len as u16;
                }
            } else if (window_rows - DISPLAY_BOTTOM_LINE_OFFSET as u16) != cursor_pos_row
                && line_count != (cursor_pos_row + 1) as usize
            {
                execute!(stdout(), MoveDown(1))?;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64 + 1, before_cursor_pos_col);

                // reset cursor position when line length is shorter than cursor position
                let now_line = lines.line(now_line_idx + 1);
                next_line_len = utils::line::get_stripped_line_length(now_line);
                next_line_len = next_line_len.saturating_sub(1);
                if cursor_pos_col > next_line_len as u16 {
                    col_diff = cursor_pos_col - next_line_len as u16;
                }
            }

            if col_diff > 0 {
                let _shadow_cursor_pos_col = display_lines.shadow_cursor_pos.1;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, _shadow_cursor_pos_col);
                execute!(stdout(), MoveLeft(col_diff))?;
            }

            // fix col position for shadow cursor
            let shadow_cursor_col_diff = is_required_correction_cursor_col(
                cursor_pos_col as u64,
                before_cursor_pos_col,
                next_line_len as u64 + 1,
            );
            if shadow_cursor_col_diff > 0 {
                let mut d = shadow_cursor_col_diff as i16 - cursor_pos_col as i16;
                if d < 0 {
                    d = 0;
                }
                execute!(stdout(), MoveRight(d as u16))?;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, before_cursor_pos_col);
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('k') | KeyCode::Up,
            ..
        }) => {
            let mut col_diff = 0;
            let mut prev_line_len = 0;
            // save cursor position before move
            let before_cursor_pos_col = display_lines.shadow_cursor_pos.1;

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
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64 - 1, before_cursor_pos_col);

                // TODO: first line
                let now_line = lines.line(now_line_idx - 1);
                prev_line_len = utils::line::get_stripped_line_length(now_line);
                prev_line_len = prev_line_len.saturating_sub(1);
                if cursor_pos_col > prev_line_len as u16 {
                    col_diff = cursor_pos_col - prev_line_len as u16;
                }
            } else if 0 != cursor_pos_row {
                execute!(stdout(), MoveUp(1))?;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64 - 1, before_cursor_pos_col);

                // reset cursor position when line length is shorter than cursor position
                let now_line = lines.line(now_line_idx - 1);
                prev_line_len = utils::line::get_stripped_line_length(now_line);
                prev_line_len = prev_line_len.saturating_sub(1);
                if cursor_pos_col > prev_line_len as u16 {
                    col_diff = cursor_pos_col - prev_line_len as u16;
                }
            }

            if col_diff > 0 {
                let _shadow_cursor_pos_col = display_lines.shadow_cursor_pos.1;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, _shadow_cursor_pos_col);
                execute!(stdout(), MoveLeft(col_diff))?;
            }

            // fix col position for shadow cursor
            let shadow_cursor_col_diff = is_required_correction_cursor_col(
                cursor_pos_col as u64,
                before_cursor_pos_col,
                prev_line_len as u64 + 1,
            );
            if shadow_cursor_col_diff > 0 {
                let mut d = shadow_cursor_col_diff as i16 - cursor_pos_col as i16;
                if d < 0 {
                    d = 0;
                }
                execute!(stdout(), MoveRight(d as u16))?;
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, before_cursor_pos_col);
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('h') | KeyCode::Left,
            ..
        }) => {
            if cursor_pos_col > 0 {
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, cursor_pos_col as u64 - 1);
                execute!(stdout(), MoveLeft(1))?
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('l') | KeyCode::Right,
            ..
        }) => {
            if line_len as u16 - 1 > cursor_pos_col {
                *display_lines.shadow_cursor_pos_mut() = (cursor_pos_row as u64, cursor_pos_col as u64 + 1);
                execute!(stdout(), MoveRight(1))?
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }) => {
            let mut scroll_offset: u16 = CURSOR_JUMP_OFFSET;
            let display_line_start = if display_lines.start <= CURSOR_JUMP_OFFSET as u64 {
                scroll_offset = if display_lines.start == 0 {
                    0
                } else {
                    CURSOR_JUMP_OFFSET - display_lines.start as u16
                };
                1
            } else {
                display_lines.start - CURSOR_JUMP_OFFSET as u64 + 1
            };

            if scroll_offset > 0 {
                execute!(stdout(), ScrollDown(scroll_offset))?;

                *display_lines.start_mut() = display_line_start - 1;
                *display_lines.end_mut() = display_line_start + window_rows as u64 - STATUS_LINE_OFFSET as u64 - 2;

                execute!(stdout(), SavePosition, Clear(ClearType::All))?;
                re_render_display_lines(lines, display_line_start as usize, window_rows)?;
                execute!(stdout(), RestorePosition)?;
            }
            let mut jump_offset = CURSOR_JUMP_OFFSET - scroll_offset;
            if jump_offset > 0 {
                let check_offset = 0; //cursor_pos_row - jump_offset;
                if jump_offset > cursor_pos_row {
                    jump_offset -= cursor_pos_row;
                }
                if jump_offset > 0 {
                    execute!(stdout(), MoveUp(jump_offset))?;
                }

                // for debug
                if false {
                    execute!(
                        stdout(),
                        SavePosition,
                        Print(format!(
                            "pos={:?},sc={:?},jmp={:?},check={:?},lines.end={:?},line_start={:?},",
                            cursor_pos_row,
                            scroll_offset,
                            jump_offset,
                            check_offset,
                            display_lines.end,
                            display_line_start
                        )),
                        RestorePosition
                    )?
                }
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }) => {
            let mut scroll_offset: u16 = CURSOR_JUMP_OFFSET;
            let mut display_line_end = now_line_idx + CURSOR_JUMP_OFFSET as usize + window_rows as usize
                - STATUS_LINE_OFFSET
                - cursor_pos_row as usize;
            if display_line_end > line_count - 1 {
                scroll_offset = CURSOR_JUMP_OFFSET - (display_line_end - line_count - 1) as u16;
                display_line_end = line_count - 1;
            }
            if display_line_end == line_count - 1 && display_lines.end == display_line_end as u64 {
                scroll_offset = 0;
            }
            if scroll_offset > 0 {
                execute!(stdout(), ScrollUp(scroll_offset))?;

                execute!(stdout(), SavePosition, Clear(ClearType::All))?;
                let line_start_num = now_line_idx + scroll_offset as usize;
                let line_start_idx = line_start_num - 1;
                re_render_display_lines(lines, line_start_num, window_rows)?;
                execute!(stdout(), RestorePosition)?;
                *display_lines.start_mut() = line_start_idx as u64;
                *display_lines.end_mut() = display_line_end as u64;
                debug!("Ctrl-d: scroll_offset sc={:?}, line_end={:?}, count={:?}, display_lines={:?}", scroll_offset, display_line_end, line_count - 1, display_lines);
            }
            let mut jump_offset = CURSOR_JUMP_OFFSET - scroll_offset;
            if jump_offset > 0 {
                let check_offset = display_lines.start as u16 + cursor_pos_row + jump_offset;
                if check_offset > line_count as u16 {
                    jump_offset = line_count as u16 - display_lines.start as u16 - cursor_pos_row - 1;
                    debug!("Ctrl-d: over line_count jmp={:?}", jump_offset);
                } else if check_offset > display_line_end as u16 {
                    jump_offset = window_rows - cursor_pos_row - STATUS_LINE_OFFSET as u16 - 1;
                    debug!("Ctrl-d: jmp={:?}", jump_offset);
                }
                if jump_offset > 0 {
                    execute!(stdout(), MoveDown(jump_offset))?;
                }

                debug!(
                    "pos={:?},sc={:?},jmp={:?},check={:?},lines.end={:?},line_end={:?},",
                    cursor_pos_row, scroll_offset, jump_offset, check_offset, display_lines.end, display_line_end
                );
            }
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('/'),
            ..
        }) => {
            return_search_word_input_mode = true;
            *display_lines.cursor_pos_mut() = (cursor_pos_row as u64, cursor_pos_col as u64);
            clear_search_line()?;
            execute!(stdout(), SavePosition, MoveTo(0, window_rows + 1), Print("/"))?;
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('n'),
            ..
        }) => {
            // jump next search result
            if search_result.clone().exists_match() {
                let now_position_row = now_line_idx as u64 + 2;
                let now_position_col = display_lines.cursor_pos.1;
                if let Some((lnum, _lcol)) = search_result.get_near_line((now_position_row, now_position_col)) {
                    let lcol = _lcol;
                    // jump to result line
                    execute!(stdout(), RestorePosition, SavePosition, Clear(ClearType::All))?;

                    re_render_display_lines(lines, lnum as usize, window_rows)?;

                    *display_lines.start_mut() = lnum - 1;
                    *display_lines.end_mut() = lnum - 1;
                    execute!(stdout(), RestorePosition, MoveTo(0, 0), MoveTo(lcol as u16, 0))?;
                };
            };

            render_search_line(search_result)?;
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('N'),
            ..
        }) => {
            // jump next search result
            if search_result.clone().exists_match() {
                let now_position_row = now_line_idx as u64 + 2;
                let now_position_col = display_lines.cursor_pos.1;
                if let Some((lnum, _lcol)) =
                    search_result.get_near_line_with_previous((now_position_row, now_position_col))
                {
                    let lcol = _lcol;
                    // jump to result line
                    execute!(stdout(), RestorePosition, SavePosition, Clear(ClearType::All))?;

                    re_render_display_lines(lines, lnum as usize, window_rows)?;

                    *display_lines.start_mut() = lnum - 1;
                    *display_lines.end_mut() = lnum - 1;
                    execute!(stdout(), RestorePosition, MoveTo(0, 0), MoveTo(lcol as u16, 0))?;
                };
            };

            render_search_line(search_result)?;
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
        shadow_cursor_pos: (0, 0),
    };

    for idx in 0..(window_rows - STATUS_LINE_OFFSET as u16) {
        *display_lines.end_mut() = idx as u64;
        // NOTE: use format, because debug print
        let disp = format!("{}", lines.line(idx as usize));
        execute!(stdout(), MoveTo(0, idx), Print(disp))?;
        if idx >= line_count as u16 - 1 {
            break;
        }
    }
    execute!(stdout(), MoveTo(0, 0), SavePosition)?;

    loop {
        let (cursor_pos_col, cursor_pos_row) = position()?;
        let now_line_num = if is_search_word_input_mode {
            display_lines.start + 1 + display_lines.cursor_pos.0
        } else {
            display_lines.start + 1 + cursor_pos_row as u64
        };
        let now_line_idx = now_line_num as usize - 1;

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

            if let Event::Key(KeyEvent { code: KeyCode::Esc, .. }) = event {
                debug!("exit");
                break;
            }

            is_search_word_input_mode = handler_display_input_mode(
                &mut display_lines,
                window_rows,
                cursor_pos_row,
                cursor_pos_col,
                now_line_idx,
                line_count,
                &lines,
                &event,
                is_search_word_input_mode,
                &mut search_result,
            )?;
        }
    }

    Ok(())
}
