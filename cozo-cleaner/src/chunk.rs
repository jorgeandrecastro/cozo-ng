/// Splits `text` into chunks of at most `size` characters, each starting about `overlap` characters
/// before the end of the previous one.
///
/// Cuts prefer, in this order: a paragraph break, a sentence end, any whitespace, looking only in the
/// last 40% of the window so chunks stay close to `size`. Chunks never start in the middle of a word.
/// `size` must be > 0 and `overlap` < `size`.
pub fn chunk_text(text: &str, size: usize, overlap: usize) -> Vec<String> {
    assert!(size > 0 && overlap < size, "chunk_text: need size > 0 and overlap < size");
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = vec![];
    let mut start = 0;
    while start < n {
        let hard_end = (start + size).min(n);
        let cut = if hard_end == n { n } else { best_cut(&chars, start, hard_end, size) };
        let piece: String = chars[start..cut].iter().collect();
        let piece = piece.trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }
        if cut >= n {
            break;
        }
        let mut next = cut.saturating_sub(overlap);
        if next <= start {
            next = cut;
        }
        // do not begin in the middle of a word
        while next < cut && next > 0 && !chars[next - 1].is_whitespace() {
            next += 1;
        }
        start = next;
    }
    out
}

fn best_cut(chars: &[char], start: usize, hard_end: usize, size: usize) -> usize {
    let floor = start + size * 6 / 10;
    let floor = floor.max(start + 1);
    // paragraph break
    for i in (floor..hard_end).rev() {
        if chars[i] == '\n' && i > 0 && chars[i - 1] == '\n' {
            return i + 1;
        }
    }
    // sentence end
    for i in (floor..hard_end).rev() {
        if matches!(chars[i - 1], '.' | '!' | '?' | '。' | '！' | '？') && chars[i].is_whitespace() {
            return i;
        }
    }
    // any whitespace
    for i in (floor..hard_end).rev() {
        if chars[i].is_whitespace() {
            return i;
        }
    }
    hard_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk_text("  hello world ", 100, 10), vec!["hello world"]);
        assert!(chunk_text("   ", 100, 10).is_empty());
    }

    #[test]
    fn respects_size_and_overlaps() {
        let text = "Alpha beta gamma. Delta epsilon zeta eta. Theta iota kappa lambda. Mu nu xi omicron pi rho sigma tau.";
        let chunks = chunk_text(text, 40, 12);
        assert!(chunks.len() > 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 40), "{chunks:?}");
        // consecutive chunks share some words
        for w in chunks.windows(2) {
            let last = w[0].split_whitespace().last().unwrap();
            assert!(w[1].contains(last) || w[0].ends_with('.'), "{w:?}");
        }
        // nothing is lost: every word of the text appears somewhere
        for word in text.split_whitespace() {
            assert!(chunks.iter().any(|c| c.contains(word)), "missing {word}");
        }
    }

    #[test]
    fn no_overlap_partitions_the_text() {
        let text = "one two three four five six seven eight nine ten";
        let chunks = chunk_text(text, 15, 0);
        assert_eq!(chunks.join(" "), text);
    }

    #[test]
    fn long_word_is_hard_cut() {
        let chunks = chunk_text(&"x".repeat(25), 10, 0);
        assert_eq!(chunks.iter().map(|c| c.len()).collect::<Vec<_>>(), vec![10, 10, 5]);
    }

    #[test]
    fn handles_multibyte_text() {
        let chunks = chunk_text("日本語のテキスト。二番目の文です。三番目の文です。", 12, 2);
        assert!(chunks.iter().all(|c| c.chars().count() <= 12));
    }
}
