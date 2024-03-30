pub fn get_stripped_line_length(line: ropey::RopeSlice) -> usize {
    let result = if let Some(v) = line.as_str() {
        v.trim_end().len()
    } else {
        0
    };
    result
}
