// 1-dimensional span of source locations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span1 {
    pub start_ix: usize,
    pub end_ix: usize, // exclusive
}

// 2-dimensional span of source locations
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span2 {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize, // inclusive
    pub end_col: usize,  // exclusive
}

use std::fmt;
use std::fmt::Display;

impl Display for Span2 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "line {}, column {} to line {}, column {}",
            self.start_line, self.start_col, self.end_line, self.end_col
        )
    }
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    newlines: Vec<usize>,
    len: usize,
}

pub fn file_info(s: &str) -> FileInfo {
    FileInfo {
        newlines: s
            .char_indices()
            .filter(|(_i, c)| *c == '\n')
            .map(|(i, _c)| i)
            .collect(),
        len: s.len(),
    }
}

pub fn span1_to_span2(info: &FileInfo, offsets: Span1) -> Span2 {
    let mut v = vec![0];
    v.extend(info.newlines.iter().map(|ix| ix + 1));
    v.push(info.len);

    let (start_line, start_col) = offset_to_line_col(&v, offsets.start_ix);
    let (end_line, end_col) = offset_to_line_col(&v, offsets.end_ix - 1);
    Span2 {
        start_line,
        start_col,
        end_line,
        end_col: end_col + 1,
    }
}

fn offset_to_line_col(newlines: &[usize], offset: usize) -> (usize, usize) {
    let mut win = newlines.windows(2).enumerate();
    while let Some((line, &[start, end])) = win.next() {
        if start <= offset && offset < end {
            return (line + 1, offset - start);
        }
    }
    panic!("internal error: offset_to_line_col. Send this to the professor");
}
