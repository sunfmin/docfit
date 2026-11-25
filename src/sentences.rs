//! Decides `docfit fmt`: a document's prose broken at its sentences, one
//! sentence per line, so two changes to different sentences of a paragraph
//! touch different lines. Pure: the text arrives as an argument.

/// Ends a sentence when a space and more text follow it.
const ENDS: [char; 3] = ['.', '?', '!'];
/// Ends a sentence when more text follows it, with or without a space.
const WIDE_ENDS: [char; 3] = ['。', '？', '！'];
/// May close a sentence after its end, before the space.
const CLOSERS: [char; 11] = [')', ']', '"', '\'', '”', '’', '」', '』', '）', '*', '_'];

/// `text` with every prose line broken after each sentence end that has more
/// of the line after it; a line it breaks keeps its indent, quote and list
/// marker's width on each new line. Front matter, fenced code, tables,
/// headings, HTML and link definitions come out byte for byte. A second
/// run changes nothing. With it, the number (from 1) of the first line it
/// broke: `None` when `text` was formatted already.
pub(crate) fn format(text: &str) -> (String, Option<usize>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let tables = tables(&lines);
    let mut out = String::with_capacity(text.len());
    let mut front = lines.first().is_some_and(|l| l.trim_end() == "---");
    let mut fence: Option<(char, usize)> = None;
    let mut first = None;
    for (n, line) in lines.iter().enumerate() {
        let (body, end) = split_end(line);
        let (prefix, rest) = container(body);
        if front {
            front = n == 0 || !matches!(body.trim_end(), "---" | "...");
        } else if let Some((c, len)) = fence {
            if run(rest.trim_start(), c) >= len
                && rest.trim_start().trim_matches(c).trim().is_empty()
            {
                fence = None;
            }
        } else if let Some(open) = opening(rest.trim_start()) {
            fence = Some(open);
        } else if !tables[n] && !verbatim(rest) {
            out.push_str(&prefix.0);
            let again = if end.is_empty() { "\n" } else { end };
            let sentences = sentences(rest);
            if sentences.len() > 1 {
                first = first.or(Some(n + 1));
            }
            for (i, sentence) in sentences.iter().enumerate() {
                if i > 0 {
                    out.push_str(again);
                    out.push_str(&prefix.1);
                }
                out.push_str(sentence);
            }
            out.push_str(end);
            continue;
        }
        out.push_str(line);
    }
    (out, first)
}

/// A line's text and its line ending.
fn split_end(line: &str) -> (&str, &str) {
    let body = line.trim_end_matches(['\n', '\r']);
    (body, &line[body.len()..])
}

/// A line's container prefix (indent, quote markers, list marker), as
/// written and as a broken line's later lines repeat it; and the rest.
fn container(body: &str) -> ((String, String), &str) {
    let mut at = body.len() - body.trim_start().len();
    while body[at..].starts_with('>') {
        at += 1;
        at += body[at..].len() - body[at..].trim_start().len();
    }
    let marker = list_marker(&body[at..]);
    let written = &body[..at + marker];
    let again = format!("{}{}", &body[..at], " ".repeat(marker));
    ((written.to_string(), again), &body[at + marker..])
}

/// The length of a list marker and its spaces at the start of `s`, or 0.
fn list_marker(s: &str) -> usize {
    let digits = s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let mark = match s[digits..].chars().next() {
        Some('-' | '*' | '+') if digits == 0 => 1,
        Some('.' | ')') if (1..=9).contains(&digits) => digits + 1,
        _ => return 0,
    };
    let after = &s[mark..];
    let spaces = after.len() - after.trim_start_matches(' ').len();
    if spaces > 0 || after.is_empty() {
        mark + spaces
    } else {
        0
    }
}

/// How many `c` start `s`.
fn run(s: &str, c: char) -> usize {
    s.chars().take_while(|&x| x == c).count()
}

/// The fence `s` opens, if it opens one.
fn opening(s: &str) -> Option<(char, usize)> {
    ['`', '~']
        .into_iter()
        .map(|c| (c, run(s, c)))
        .find(|&(_, len)| len >= 3)
}

/// Whether the rest of a line is not prose: blank, a heading, a table row,
/// HTML, a link definition, or a line that would start a block of its own.
fn verbatim(rest: &str) -> bool {
    let rest = rest.trim_start();
    rest.is_empty() || starts_block(rest) || (rest.starts_with('[') && rest.contains("]:"))
}

/// Whether text at the start of a line would be read as something other
/// than a paragraph's line: so a sentence is never broken before it.
fn starts_block(s: &str) -> bool {
    s.starts_with(['#', '>', '|', '<'])
        || opening(s).is_some()
        || list_marker(s) > 0
        || (!s.is_empty() && s.trim_end().chars().all(|c| c == '=' || c == '-'))
}

/// Each block of lines between blank lines that holds a table's delimiter
/// row is a table, every line of it.
fn tables(lines: &[&str]) -> Vec<bool> {
    lines
        .split_inclusive(|l| l.trim().is_empty())
        .flat_map(|block| {
            let table = block.iter().any(|l| delimiter_row(l));
            std::iter::repeat_n(table, block.len())
        })
        .collect()
}

/// `|---|:--:|`, or `--- | ---`: a table's delimiter row.
fn delimiter_row(line: &str) -> bool {
    let line = line.trim();
    line.contains('|')
        && line.contains('-')
        && line
            .chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

/// `rest` cut after each sentence end that has more text after it, outside
/// inline code; the spaces between two sentences dropped.
fn sentences(rest: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut from, mut code) = (0, 0);
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '`' {
            let len = 1 + run(&rest[i + 1..], '`');
            for _ in 1..len {
                chars.next();
            }
            code = match code {
                0 => len,
                open if open == len => 0,
                open => open,
            };
            continue;
        }
        if code > 0 || !(ENDS.contains(&c) || WIDE_ENDS.contains(&c)) {
            continue;
        }
        let mut cut = i + c.len_utf8();
        while let Some(&(j, closer)) = chars.peek().filter(|(_, x)| CLOSERS.contains(x)) {
            cut = j + closer.len_utf8();
            chars.next();
        }
        let after = &rest[cut..];
        let next = after.trim_start_matches([' ', '\t']);
        let spaced = next.len() < after.len();
        if next.trim_end().is_empty() || (!spaced && !WIDE_ENDS.contains(&c)) || starts_block(next)
        {
            continue;
        }
        out.push(&rest[from..cut]);
        from = rest.len() - next.len();
    }
    out.push(&rest[from..]);
    out
}

#[cfg(test)]
mod tests {
    /// The formatted text alone.
    fn format(text: &str) -> String {
        super::format(text).0
    }

    #[test]
    fn english_breaks_after_each_sentence() {
        assert_eq!(
            format("One. Two? Three! Four.\n"),
            "One.\nTwo?\nThree!\nFour.\n"
        );
    }

    #[test]
    fn chinese_breaks_after_each_sentence_with_or_without_a_space() {
        assert_eq!(
            format("第一句。第二句？ 第三句！第四句\n"),
            "第一句。\n第二句？\n第三句！\n第四句\n"
        );
    }

    #[test]
    fn an_end_without_a_space_after_it_is_no_sentence_end() {
        let text = "See docfit.toml and 3.5, or a.b?c!d.\n";
        assert_eq!(format(text), text);
    }

    #[test]
    fn a_closer_stays_with_its_sentence() {
        assert_eq!(
            format("(See it.) **Bold.** \"Quoted.\" Next.\n"),
            "(See it.)\n**Bold.**\n\"Quoted.\"\nNext.\n"
        );
    }

    #[test]
    fn a_list_item_keeps_its_marker_and_its_later_lines_its_width() {
        assert_eq!(
            format("- One. Two.\n  10. Three. Four.\n> Five. Six.\n"),
            "- One.\n  Two.\n  10. Three.\n      Four.\n> Five.\n> Six.\n"
        );
    }

    #[test]
    fn code_tables_headings_and_front_matter_come_out_byte_for_byte() {
        let text = "---\ntitle: A. B.\n---\n# A. B.\n\n```sh\necho a. b.\n```\n\n  ~~~~\n  a. b.\n  ~~~\n  ~~~~\n\n| a. b. | c. d. |\n|---|---|\n| e. f. | g. h. |\n\na. b | c. d\n--- | ---\n\n<p>A. B.</p>\n[x]: http://x \"A. B.\"\n";
        assert_eq!(format(text), text);
    }

    #[test]
    fn inline_code_is_never_broken() {
        let text = "Run `a. b` and ``c. ` d.`` then.\n";
        assert_eq!(format(text), text);
    }

    #[test]
    fn a_sentence_is_never_broken_before_what_would_start_a_block() {
        let text = "Step one. 1. Two. - three. # four. > five. | six.\n";
        assert_eq!(
            format(text),
            "Step one. 1.\nTwo. - three. # four. > five. | six.\n"
        );
    }

    #[test]
    fn a_second_run_changes_nothing() {
        let text =
            "---\na: b\n---\nOne. Two.  \n- A. B. 1. C.\r\nD。E\n\n```\nx. y\n```\n第一。 第二。\n";
        let once = format(text);
        assert_ne!(once, text);
        assert_eq!(format(&once), once);
    }

    #[test]
    fn the_first_line_it_breaks_is_named() {
        assert_eq!(super::format("a\nb\nc. d\ne. f\n").1, Some(3));
        assert_eq!(super::format("a\n").1, None);
    }
}
