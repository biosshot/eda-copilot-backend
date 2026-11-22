//! IBIS Touchstone 2.0 (2009), pp.6-18,26; bounded S/full/common subset.
use super::*;
use std::collections::BTreeSet;
pub fn parse(content: &str, n: usize) -> Result<(Vec<f64>, Vec<Matrix>, Vec<f64>, Value)> {
    if content.len() > TEXT_BYTES {
        return Err(fail("NETWORK_BUDGET", "Touchstone text exceeds 8 MiB"));
    }
    if !content
        .bytes()
        .all(|c| c == 9 || c == 10 || c == 13 || (32..=126).contains(&c))
    {
        return Err(fail("TOUCHSTONE_INPUT", "Only standard ASCII is supported"));
    }
    let mut v2 = false;
    let mut option = false;
    let mut data = false;
    let mut ended = false;
    let mut info = false;
    let mut first = true;
    let mut seen = BTreeSet::new();
    let mut count = None;
    let mut factor = 1e9;
    let mut units = "GHz".to_string();
    let mut format = "MA".to_string();
    let mut reference = 50.;
    let mut refs = Vec::new();
    let mut ref_pending = false;
    let mut order = "21_12".to_string();
    let mut pending = Vec::new();
    let mut frequencies = Vec::new();
    let mut matrices = Vec::new();
    let width = 1 + 2 * n * n;
    let mut lines = content.split_inclusive(['\r', '\n']).peekable();
    let mut line = 0;
    while let Some(raw) = lines.next() {
        // CRLF is one terminator; lone CR and LF are also standard line endings.
        if raw.ends_with('\r') && lines.peek() == Some(&"\n") {
            lines.next();
        }
        line += 1;
        let err = |s: &str| fail("TOUCHSTONE_INPUT", format!("line {line}: {s}"));
        let text = raw.split('!').next().unwrap().trim();
        if text.is_empty() {
            continue;
        }
        if info {
            if text.eq_ignore_ascii_case("[End Information]") {
                info = false;
            }
            continue;
        }
        if ended {
            return Err(err("Data after [End]"));
        }
        if text.starts_with('[') {
            if !raw.starts_with('[') {
                return Err(err("Keyword must start in column 1"));
            }
            let close = text.find(']').ok_or_else(|| err("Unclosed keyword"))?;
            let key = text[1..close].to_ascii_lowercase();
            let arg = text[close + 1..].trim();
            if text[1..close].trim() != &text[1..close]
                || (!arg.is_empty() && !text.as_bytes()[close + 1].is_ascii_whitespace())
            {
                return Err(err("Malformed keyword"));
            }
            if !pending.is_empty() {
                return Err(err("Incomplete network record"));
            }
            if key == "version" {
                if !first || arg != "2.0" {
                    return Err(err(
                        "[Version] 2.0 must be first and unique; 1.1/2.1 unsupported",
                    ));
                }
                v2 = true;
                first = false;
                seen.insert(key);
                continue;
            }
            first = false;
            if !v2 || !option {
                return Err(err(
                    "2.0 keywords require preceding [Version] and option line",
                ));
            }
            if !seen.contains("number of ports") && key != "number of ports" {
                return Err(err("[Number of Ports] must follow option line"));
            }
            if !seen.insert(key.clone()) {
                return Err(err("Duplicate keyword"));
            }
            if data && key != "end" {
                return Err(err("Header/unsupported block after [Network Data]"));
            }
            ref_pending = false;
            match key.as_str() {
                "number of ports" => {
                    if arg.parse::<usize>().ok() != Some(n) {
                        return Err(err("Port count conflicts with Resource.ports"));
                    }
                }
                "number of frequencies" => {
                    let f = arg
                        .parse::<usize>()
                        .map_err(|_| err("Invalid frequency count"))?;
                    admit_table(n, f)?;
                    count = Some(f);
                }
                "two-port data order" => {
                    if n != 2 || !matches!(arg, "21_12" | "12_21") {
                        return Err(err("Invalid two-port order"));
                    }
                    order = arg.to_string();
                }
                "matrix format" => {
                    if !arg.eq_ignore_ascii_case("full") {
                        return Err(err("Only Full matrices supported"));
                    }
                }
                "reference" => {
                    for s in arg.split_whitespace() {
                        refs.push(s.parse::<f64>().map_err(|_| err("Invalid reference"))?);
                    }
                    if refs.len() > n {
                        return Err(err("Too many reference values"));
                    }
                    ref_pending = refs.len() < n;
                }
                "begin information" => {
                    if !arg.is_empty() {
                        return Err(err("Unexpected information argument"));
                    }
                    info = true;
                }
                "network data" => {
                    if !arg.is_empty()
                        || count.is_none()
                        || n == 2 && !seen.contains("two-port data order")
                    {
                        return Err(err("Missing counts/order or unexpected argument"));
                    }
                    if seen.contains("reference") && refs.len() != n {
                        return Err(err("[Reference] must specify exactly one value per port"));
                    }
                    data = true;
                }
                "end" => {
                    if !data || !arg.is_empty() {
                        return Err(err("Premature/invalid [End]"));
                    }
                    ended = true;
                }
                _ => {
                    return Err(err(&format!(
                        "Unsupported physical keyword [{key}]; no mixed-mode/noise/triangular data"
                    )))
                }
            }
            continue;
        }
        first = false;
        if let Some(rest) = text.strip_prefix('#') {
            if !pending.is_empty() || data && !option {
                return Err(err("Option line after data"));
            }
            if option {
                continue;
            } // Standard: additional option lines are ignored.
            if v2 && !seen.contains("version") {
                return Err(err("Missing version"));
            }
            let mut tokens = rest.split_whitespace();
            let mut options = BTreeSet::new();
            while let Some(t) = tokens.next() {
                let t = t.to_ascii_uppercase();
                let category = match t.as_str() {
                    "HZ" | "KHZ" | "MHZ" | "GHZ" => {
                        units = t.clone();
                        factor = match t.as_str() {
                            "HZ" => 1.,
                            "KHZ" => 1e3,
                            "MHZ" => 1e6,
                            _ => 1e9,
                        };
                        "unit"
                    }
                    "S" => "parameter",
                    "Y" | "Z" | "H" | "G" => {
                        return Err(err("Only S parameters supported in Touchstone"))
                    }
                    "RI" | "MA" | "DB" => {
                        format = t.clone();
                        "format"
                    }
                    "R" => {
                        reference = tokens
                            .next()
                            .ok_or_else(|| err("Missing R value"))?
                            .parse::<f64>()
                            .map_err(|_| err("Invalid R value"))?;
                        "reference"
                    }
                    _ => return Err(err("Unknown option token")),
                };
                if !options.insert(category) {
                    return Err(err("Duplicate option"));
                }
            }
            if !reference.is_finite() || reference <= 0. {
                return Err(err("Reference must be finite positive real"));
            }
            option = true;
            continue;
        }
        if !option || v2 && !data && !ref_pending {
            return Err(err("Numeric data before option/[Network Data]"));
        }
        if ref_pending {
            for s in text.split_whitespace() {
                if refs.len() >= n {
                    return Err(err("Too many reference values"));
                }
                refs.push(s.parse::<f64>().map_err(|_| err("Invalid reference"))?);
            }
            ref_pending = refs.len() < n;
            continue;
        }
        data = true;
        let before = matrices.len();
        for token in text.split_whitespace() {
            let number = token
                .parse::<f64>()
                .map_err(|_| err("Invalid numeric token"))?;
            if !number.is_finite() {
                return Err(err("Nonfinite number"));
            }
            pending.push(number);
            if pending.len() == width {
                admit_table(n, matrices.len() + 1)?;
                if count.is_some_and(|f| matrices.len() >= f) {
                    return Err(err("More records than [Number of Frequencies]"));
                }
                let f = pending[0] * factor;
                if !f.is_finite() || f < 0. || frequencies.last().is_some_and(|p| f <= *p) {
                    return Err(err(
                        "Frequency must be finite, nonnegative, strictly increasing",
                    ));
                }
                let mut m = zero(n, n);
                for k in 0..n * n {
                    let x = pending[1 + 2 * k];
                    let y = pending[2 + 2 * k];
                    let z = if format == "RI" {
                        C::new(x, y)
                    } else {
                        if format == "MA" && x < 0. {
                            return Err(err("Negative magnitude"));
                        }
                        let mag = if format == "DB" {
                            10_f64.powf(x / 20.)
                        } else {
                            x
                        };
                        C::new(mag * y.to_radians().cos(), mag * y.to_radians().sin())
                    };
                    if !z.abs().is_finite() {
                        return Err(err("Nonfinite converted coefficient"));
                    }
                    let (i, j) = if n == 2 && order == "21_12" {
                        (k % 2, k / 2)
                    } else {
                        (k / n, k % n)
                    };
                    m[i][j] = z;
                }
                frequencies.push(f);
                matrices.push(m);
                pending.clear();
            }
        }
        // Original 1.0 one/two-port files use one complete record per line. This
        // also prevents a following 5-number legacy noise record being reinterpreted.
        if !v2 && n <= 2 && (!pending.is_empty() || matrices.len() != before + 1) {
            return Err(err(
                "1.0 one/two-port record must occupy one line; noise unsupported",
            ));
        }
    }
    if info
        || !pending.is_empty()
        || !option
        || matrices.is_empty()
        || v2 && (!ended || count != Some(matrices.len()))
    {
        return Err(fail(
            "TOUCHSTONE_INPUT",
            "Incomplete header/record, count mismatch, unterminated information or missing [End]",
        ));
    }
    if refs.is_empty() {
        refs = vec![reference; n];
    }
    if refs.len() != n || refs.iter().any(|r| !r.is_finite() || *r <= 0.) {
        return Err(fail("TOUCHSTONE_INPUT", "Invalid per-port reference"));
    }
    Ok((
        frequencies,
        matrices,
        refs,
        json!({"format":"touchstone","version":if v2{"2.0"}else{"1.0"},"units":units,"coefficientFormat":format,"optionReferenceOhm":reference,"twoPortDataOrder":if n==2{order.as_str()}else{"row-major"},"matrixFormat":"Full","information":"explicitly ignored metadata","defaults":"GHz S MA R 50 for omitted option tokens"}),
    ))
}
