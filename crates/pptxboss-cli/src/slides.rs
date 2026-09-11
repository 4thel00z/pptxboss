//! `--slides RANGE`: which slides a command works on.

/// Parses a comma-separated list of one-based slide numbers and low-high
/// ranges (`1-3,7`) into zero-based indices in the written order.
/// Duplicates are kept. Errors name the offending item and the slide count.
pub fn parse_slides(spec: &str, count: usize) -> Result<Vec<usize>, String> {
    let mut indices = Vec::new();
    for item in spec.split(',') {
        let item = item.trim();
        let Some((low, high)) = item.split_once('-') else {
            indices.push(parse_number(item, item, count)?);
            continue;
        };
        let low = parse_number(low, item, count)?;
        let high = parse_number(high, item, count)?;
        if low > high {
            return Err(format!(
                "not a slide or range: \"{item}\" (ranges are low-high)"
            ));
        }
        indices.extend(low..=high);
    }
    Ok(indices)
}

fn parse_number(text: &str, item: &str, count: usize) -> Result<usize, String> {
    let number: usize = text
        .trim()
        .parse()
        .map_err(|_| format!("not a slide or range: \"{item}\""))?;
    if number == 0 {
        return Err("slide 0 does not exist (slides count from 1)".to_string());
    }
    if number > count {
        let plural = match count {
            1 => "",
            _ => "s",
        };
        return Err(format!(
            "slide {number} out of range (deck has {count} slide{plural})"
        ));
    }
    Ok(number - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_ranges_in_written_order() {
        assert_eq!(parse_slides("3,1", 3).unwrap(), [2, 0]);
        assert_eq!(parse_slides("1-3", 5).unwrap(), [0, 1, 2]);
        assert_eq!(parse_slides(" 2 , 4-5 ", 5).unwrap(), [1, 3, 4]);
        assert_eq!(parse_slides("2,2", 2).unwrap(), [1, 1]);
        assert_eq!(parse_slides("4-4", 4).unwrap(), [3]);
    }

    #[test]
    fn bad_items_name_the_item_and_the_count() {
        assert_eq!(
            parse_slides("", 3).unwrap_err(),
            "not a slide or range: \"\""
        );
        assert_eq!(
            parse_slides("1,,2", 3).unwrap_err(),
            "not a slide or range: \"\""
        );
        assert_eq!(
            parse_slides("a", 3).unwrap_err(),
            "not a slide or range: \"a\""
        );
        assert_eq!(
            parse_slides("2-", 3).unwrap_err(),
            "not a slide or range: \"2-\""
        );
        assert_eq!(
            parse_slides("0", 3).unwrap_err(),
            "slide 0 does not exist (slides count from 1)"
        );
        assert_eq!(
            parse_slides("4", 3).unwrap_err(),
            "slide 4 out of range (deck has 3 slides)"
        );
        assert_eq!(
            parse_slides("2", 1).unwrap_err(),
            "slide 2 out of range (deck has 1 slide)"
        );
        assert_eq!(
            parse_slides("3-1", 3).unwrap_err(),
            "not a slide or range: \"3-1\" (ranges are low-high)"
        );
    }
}
