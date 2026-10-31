pub(super) fn report_status(as_output: bool, message: std::fmt::Arguments<'_>) {
    if as_output {
        println!("{message}");
    } else {
        tracing::info!("{message}");
    }
}

pub(super) fn table<const N: usize>(headers: [&str; N], rows: Vec<[String; N]>) -> String {
    let widths: [usize; N] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| console::measure_text_width(&row[column]))
            .chain(std::iter::once(console::measure_text_width(
                headers[column],
            )))
            .max()
            .unwrap()
    });
    let mut text = String::new();
    for row in [
        headers.map(str::to_owned),
        widths.map(|width| "-".repeat(width)),
    ]
    .into_iter()
    .chain(rows)
    {
        text.push_str("  ");
        for (column, cell) in row.iter().enumerate() {
            text.push_str(cell);
            if column + 1 < N {
                text.push_str(&" ".repeat(widths[column] - console::measure_text_width(cell) + 2));
            }
        }
        text.push('\n');
    }
    text
}

pub(super) fn fields(rows: &[(&str, String)]) -> String {
    let width = rows
        .iter()
        .map(|(name, _)| console::measure_text_width(name))
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for (name, value) in rows {
        text.push_str("  ");
        text.push_str(name);
        text.push_str(&" ".repeat(width - console::measure_text_width(name) + 2));
        text.push_str(value);
        text.push('\n');
    }
    text
}
