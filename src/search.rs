use std::io;
use log::debug;
use grep::matcher::Matcher;
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::SearcherBuilder;

#[derive(Clone, Debug)]
pub struct SearchResult<'a> {
    pub filename: &'a str,
    pub word: String,
    pub word_vec: Vec<char>,          // input temporary search word
    pub match_lines: Vec<(u64, u64)>, // (line number, position)
    pub now_idx: Option<usize>,
}

impl SearchResult<'_> {
    pub fn new(filename: &'_ str) -> SearchResult<'_> {
        SearchResult {
            filename,
            word: String::new(),
            word_vec: Vec::new(),
            match_lines: Vec::new(),
            now_idx: None,
        }
    }
    pub fn word_mut(&mut self) -> &mut String {
        &mut self.word
    }
    pub fn word_vec_mut(&mut self) -> &mut Vec<char> {
        &mut self.word_vec
    }
    pub fn match_lines_mut(&mut self) -> &mut Vec<(u64, u64)> {
        &mut self.match_lines
    }
    pub fn exists_match(self) -> bool {
        self.now_idx.is_some()
    }
    pub fn get_near_line(&mut self, now_pos: (u64, u64)) -> Option<(u64, u64)> {
        let mut pos = None;
        for idx in 0..self.match_lines.clone().len() {
            let (line_num, _) = self.match_lines[idx];
            if line_num >= now_pos.0 {
                pos = Some(self.match_lines[idx]);
                self.now_idx = Some(idx);
                break;
            }
        }
        pos
    }

    pub fn get_near_line_with_previous(&mut self, now_pos: (u64, u64)) -> Option<(u64, u64)> {
        let mut pos = None;
        for idx in (0..self.match_lines.clone().len()).rev() {
            let (line_num, _) = self.match_lines[idx];
            if line_num + 1 < now_pos.0 {
                pos = Some(self.match_lines[idx]);
                self.now_idx = Some(idx);
                break;
            }
        }
        pos
    }

    pub fn reset(&mut self) {
        self.word = String::new();
        self.word_vec = Vec::new();
        self.match_lines = Vec::new();
        self.now_idx = None;
    }
}

pub fn search(filename: &str, search_word: &str) -> io::Result<Vec<(u64, u64)>> {
    debug!("start search: search_word={}", search_word);
    let matcher = RegexMatcher::new(search_word).unwrap();
    let mut matches: Vec<(u64, u64)> = vec![];
    let mut searcher = SearcherBuilder::new().build();
    searcher.search_path(
        &matcher,
        filename,
        UTF8(|lnum, _line| {
            let linematch = matcher.find_at(_line.as_bytes(), 0).unwrap();
            matches.push((lnum, linematch.unwrap().start() as u64));
            Ok(true)
        }),
    )?;
    debug!("start end: search_word={}, hit={}", search_word, matches.len());
    Ok(matches)
}
