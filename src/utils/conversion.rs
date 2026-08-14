pub fn bytes_to_human_readable(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{} {}", size.round(), UNITS[unit_index])
}

// Convert a number of tokens to a human-readable string, e.g., 1.5k, 2.3M, etc.
pub fn tokens_to_human_readable(tokens: u32) -> String {
    if tokens < 1000 {
        return tokens.to_string();
    }

    const UNITS: [&str; 3] = ["", "k", "M"];
    let mut size = tokens as f64;
    let mut unit_index = 0;

    while size >= 1000.0 && unit_index < UNITS.len() - 1 {
        size /= 1000.0;
        unit_index += 1;
    }

    format!("{:.2}{}", size, UNITS[unit_index])
}
