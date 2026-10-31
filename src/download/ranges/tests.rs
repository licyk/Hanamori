use super::*;

#[test]
fn split_ranges_covers_the_file_without_empty_ranges() {
    let ranges = split_ranges(10_000_000, 8);
    assert_eq!(ranges.len(), 8);
    assert_eq!(ranges[0].0, 0);
    assert_eq!(ranges.last().unwrap().1, 9_999_999);
    for pair in ranges.windows(2) {
        assert_eq!(pair[0].1 + 1, pair[1].0);
    }
    assert_eq!(split_ranges(3, 8), vec![(0, 0), (1, 1), (2, 2)]);
    assert_ne!(
        part_path(Path::new("work"), 0),
        part_path(Path::new("work"), 1)
    );
}

#[test]
fn range_headers_must_describe_valid_intervals() {
    assert_eq!(parse_content_range("bytes 0-0/12345"), Some((0, 0, 12345)));
    for invalid in ["bytes 0-99/1", "bytes 2-1/10", "bytes 0-0/*", "garbage"] {
        assert_eq!(parse_content_range(invalid), None);
    }
}
