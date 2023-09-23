use colored::Colorize;
use std::fmt;

use crate::token::Span;

pub struct PreciseError {
    filename: String,
    heading: String,
    line: String,
    span: Span,
    text: String,
}

impl PreciseError {
    pub fn new(line: String, span: Span, heading: String, text: String) -> Self {
        Self {
            line,
            span,
            heading,
            text,
            // TODO: get actual filename
            filename: String::from("main.kya"),
        }
    }

    fn build(&self) -> String {
        let num = self.span.line.to_string();
        let len = num.len();
        let mut comment = format!(
            "{}{}{}",
            &" ".repeat(len - 1),             // padding
            &"-->".blue().bold().to_string(), // arrow
            &format!(
                " {}:{}:{}\n",
                self.filename, self.span.line, self.span.column
            ), // filename
        );

        // empty line
        sidebar(&mut comment, len, true);

        // line information
        comment.push_str(&num.blue().bold().to_string());
        comment.push_str(&" | ".blue().bold().to_string());
        comment.push_str(&self.line);
        comment.push('\n');

        // error text
        sidebar(&mut comment, len, false);
        let mut end = self.span.column + len - 1;
        if len > 1 {
            end -= len - 1;
        }
        for _ in 0..end {
            comment.push(' ');
        }
        comment.push_str(&format!("^ {}", self.text).red().bold().to_string());
        comment.push('\n');

        // empty line
        sidebar(&mut comment, len, true);

        comment
    }
}

fn sidebar(comment: &mut String, len: usize, newline: bool) {
    comment.push_str(&" ".repeat(len + 1));
    comment.push_str(&"|".blue().bold().to_string());
    if newline {
        comment.push('\n');
    }
}

impl fmt::Display for PreciseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}\n{}",
            "error".bold().red(),
            self.heading,
            self.build(),
        )
    }
}
