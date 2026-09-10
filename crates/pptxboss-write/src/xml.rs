//! Escaping and a small builder for the XML the writer emits.

/// Escapes text for character data.
pub fn text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {
                out.push_str(&format!("_x{:04X}_", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Escapes text for an attribute value delimited by double quotes.
pub fn attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

pub const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(text("a < b & c > d \"q\""), "a &lt; b &amp; c &gt; d \"q\"");
        assert_eq!(text("tab\tok\x01bad"), "tab\tok_x0001_bad");
        assert_eq!(attr("say \"hi\"\n"), "say &quot;hi&quot;&#10;");
    }
}
