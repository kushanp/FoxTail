/// Human-readable byte size (KB / MB / GB).
pub fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let n = n as f64;
    if n >= GB {
        format!("{:.2} GB", n / GB)
    } else if n >= MB {
        format!("{:.2} MB", n / MB)
    } else if n >= KB {
        format!("{:.1} KB", n / KB)
    } else {
        format!("{n} B")
    }
}

/// Integer with thousands separators.
pub fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_units() {
        assert_eq!(format_bytes(500), "500 B");
        assert!(format_bytes(2048).contains("KB"));
        assert!(format_bytes(3 * 1024 * 1024).contains("MB"));
    }

    #[test]
    fn count_commas() {
        assert_eq!(format_count(12), "12");
        assert_eq!(format_count(1234), "1,234");
        assert_eq!(format_count(1_000_000), "1,000,000");
    }
}
