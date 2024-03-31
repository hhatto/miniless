use std::io;

use grep::matcher::Matcher;
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::SearcherBuilder;

#[derive(Clone, Debug)]
pub struct SearchResult<'a> {
    pub filename: &'a str,
    pub word: String,
    pub word_vec: Vec<char>,  // input temporary search word
    pub lines: Vec<(u64, u64)>,  // (line number, position)
    now_idx: Option<usize>,
}

impl SearchResult<'_> {
    pub fn new(filename: &'_ str) -> SearchResult<'_> {
        SearchResult {
            filename,
            word: String::new(),
            word_vec: Vec::new(),
            lines: Vec::new(),
            now_idx: None,
        }
    }
    pub fn word_mut(&mut self) -> &mut String {
        &mut self.word
    }
    pub fn word_vec_mut(&mut self) -> &mut Vec<char> {
        &mut self.word_vec
    }
    pub fn lines_mut(&mut self) -> &mut Vec<(u64, u64)> {
        &mut self.lines
    }
    pub fn exists_match(self) -> bool {
        self.now_idx.is_some()
    }
    pub fn get_near_line(&mut self, now_position: u64) -> Option<(u64, u64)> {
        let mut pos = None;
        for idx in 0..self.lines.clone().len() {
            let (line_num, _) = self.lines[idx];
            if line_num >= now_position {
                pos = Some(self.lines[idx]);
                self.now_idx = Some(idx);
                break;
            }
        }
        pos
    }
    pub fn next(&mut self) -> Option<(u64, u64)> {
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
    pub fn reset(&mut self) {
        self.word = String::new();
        self.word_vec = Vec::new();
        self.lines = Vec::new();
        self.now_idx = None;
    }
}

pub fn search(filename: &str, search_word: &str) -> io::Result<Vec<(u64, u64)>> {
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
    Ok(matches)
}
