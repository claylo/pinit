#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use tracing::debug;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedLicense {
    pub spdx: String,
    pub text: String,
}

#[derive(Debug)]
pub enum LicenseError {
    UnknownSpdxId { spdx: String },
    UnterminatedDirective { spdx: String },
    MissingTemplateVar { spdx: String, name: String },
}

impl std::fmt::Display for LicenseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseError::UnknownSpdxId { spdx } => write!(f, "unknown SPDX license id: {spdx}"),
            LicenseError::UnterminatedDirective { spdx } => {
                write!(f, "unterminated SPDX template directive in {spdx} text")
            }
            LicenseError::MissingTemplateVar { spdx, name } => {
                write!(f, "missing SPDX template variable {name:?} for {spdx}")
            }
        }
    }
}

impl std::error::Error for LicenseError {}

pub fn render_spdx_license(
    spdx: &str,
    template_args: &BTreeMap<String, String>,
) -> Result<RenderedLicense, LicenseError> {
    use std::str::FromStr;

    let parsed: &dyn license::License =
        <&dyn license::License>::from_str(spdx).map_err(|_| LicenseError::UnknownSpdxId {
            spdx: spdx.to_string(),
        })?;

    let raw = parsed.text();
    let mut args = template_args.clone();
    maybe_insert_current_year(raw, &mut args);
    let expanded = expand_spdx_template(spdx, raw, &args)?;
    let expanded = replace_angle_placeholders(&expanded, &args);
    Ok(RenderedLicense {
        spdx: spdx.to_string(),
        text: expanded,
    })
}

fn expand_spdx_template(
    spdx: &str,
    template: &str,
    template_args: &BTreeMap<String, String>,
) -> Result<String, LicenseError> {
    let mut out = String::with_capacity(template.len());

    let mut idx = 0usize;
    while let Some(open_rel) = template[idx..].find("<<") {
        let open = idx + open_rel;
        out.push_str(&template[idx..open]);
        let Some(close_rel) = template[open + 2..].find(">>") else {
            return Err(LicenseError::UnterminatedDirective {
                spdx: spdx.to_string(),
            });
        };
        let close = open + 2 + close_rel;
        let directive = template[open + 2..close].trim();

        if directive.is_empty() {
            idx = close + 2;
            continue;
        }

        if directive.eq_ignore_ascii_case("beginOptional")
            || directive.eq_ignore_ascii_case("endOptional")
            || directive.to_ascii_lowercase().starts_with("beginoptional;")
            || directive.to_ascii_lowercase().starts_with("endoptional;")
        {
            idx = close + 2;
            continue;
        }

        if directive.to_ascii_lowercase().starts_with("var;")
            || directive.eq_ignore_ascii_case("var")
        {
            let value = expand_var_directive(spdx, directive, template_args)?;
            out.push_str(&value);
            idx = close + 2;
            continue;
        }

        // Unknown directives are stripped (SPDX template control codes should not appear in output).
        debug!(spdx = %spdx, directive = %directive, "strip unknown SPDX directive");
        idx = close + 2;
    }

    out.push_str(&template[idx..]);
    Ok(out)
}

fn expand_var_directive(
    spdx: &str,
    directive: &str,
    template_args: &BTreeMap<String, String>,
) -> Result<String, LicenseError> {
    let mut parts = split_semicolons(directive);
    if parts.is_empty() {
        return Ok(String::new());
    }
    if parts[0].eq_ignore_ascii_case("var") {
        parts.remove(0);
    } else if parts[0].to_ascii_lowercase().starts_with("var") {
        // e.g. "var;name=..."
        let first = parts.remove(0);
        if first != "var" {
            // ignore malformed "var..." tokens
        }
    }

    let mut name = None;
    let mut original = None;
    for part in parts {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let mut val = v.trim().to_string();
        val = unquote(&val);
        match key {
            "name" => name = Some(val),
            "original" => original = Some(val),
            _ => {}
        }
    }

    let Some(name) = name else {
        return Ok(original.unwrap_or_default());
    };

    if let Some(val) = template_args.get(&name) {
        return Ok(val.to_string());
    }

    if let Some(orig) = original {
        return Ok(orig);
    }

    Err(LicenseError::MissingTemplateVar {
        spdx: spdx.to_string(),
        name,
    })
}

fn split_semicolons(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut quote = '\0';

    for c in s.chars() {
        if in_quotes {
            if c == quote {
                in_quotes = false;
            }
            buf.push(c);
            continue;
        }

        if c == '"' || c == '\'' {
            in_quotes = true;
            quote = c;
            buf.push(c);
            continue;
        }

        if c == ';' {
            out.push(buf.trim().to_string());
            buf.clear();
            continue;
        }

        buf.push(c);
    }

    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }

    out
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..bytes.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn replace_angle_placeholders(s: &str, template_args: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut idx = 0usize;
    while let Some(open_rel) = s[idx..].find('<') {
        let open = idx + open_rel;
        out.push_str(&s[idx..open]);

        let Some(close_rel) = s[open + 1..].find('>') else {
            out.push_str(&s[open..]);
            return out;
        };
        let close = open + 1 + close_rel;
        let key = s[open + 1..close].trim();

        if let Some(val) = template_args.get(key) {
            out.push_str(val);
        } else {
            out.push('<');
            out.push_str(key);
            out.push('>');
        }

        idx = close + 1;
    }

    out.push_str(&s[idx..]);
    out
}

fn maybe_insert_current_year(template: &str, template_args: &mut BTreeMap<String, String>) {
    if template_args.contains_key("year") {
        return;
    }
    if !template_supports_year(template) {
        return;
    }
    template_args.insert("year".to_string(), current_year_string());
}

fn template_supports_year(template: &str) -> bool {
    if template.contains("<year>") {
        return true;
    }
    let lowered = template.to_ascii_lowercase();
    lowered.contains("name=\"year\"")
        || lowered.contains("name='year'")
        || lowered.contains("name=year")
}

fn current_year_string() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = (duration.as_secs() / 86_400) as i64;
    let year = civil_from_days(days).0;
    year.to_string()
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_mit_with_year_and_fullname() {
        let mut args = BTreeMap::new();
        args.insert("year".to_string(), "2025".to_string());
        args.insert("copyright holders".to_string(), "Clay".to_string());

        let rendered = render_spdx_license("MIT", &args).unwrap();
        assert!(rendered.text.contains("2025"));
        assert!(rendered.text.contains("Clay"));
        assert!(!rendered.text.contains("<<var;"));
        assert!(!rendered.text.contains("<<beginOptional>>"));
    }

    #[test]
    fn unknown_spdx_id_errors() {
        let err = render_spdx_license("Definitely-Not-A-License", &BTreeMap::new()).unwrap_err();
        assert!(matches!(err, LicenseError::UnknownSpdxId { .. }));
    }

    #[test]
    fn directive_var_uses_original_when_missing() {
        let tpl = "X <<var;name=\"missing\";original=\"DEFAULT\">> Y";
        let out = expand_spdx_template("X", tpl, &BTreeMap::new()).unwrap();
        assert_eq!(out, "X DEFAULT Y");
    }

    #[test]
    fn directive_var_errors_when_missing_without_original() {
        let tpl = "X <<var;name=\"missing\">> Y";
        let err = expand_spdx_template("X", tpl, &BTreeMap::new()).unwrap_err();
        assert!(
            matches!(err, LicenseError::MissingTemplateVar { ref name, .. } if name == "missing")
        );
    }

    #[test]
    fn unterminated_directive_errors() {
        let tpl = "X <<var;name=\"x\"";
        let err = expand_spdx_template("X", tpl, &BTreeMap::new()).unwrap_err();
        assert!(matches!(err, LicenseError::UnterminatedDirective { .. }));
    }

    #[test]
    fn split_semicolons_respects_quotes() {
        let parts = split_semicolons(r#"var;name="a;b";original='c;d';x=y"#);
        assert_eq!(parts, vec!["var", r#"name="a;b""#, "original='c;d'", "x=y"]);
    }

    #[test]
    fn replace_angle_placeholders_substitutes_known_keys() {
        let mut args = BTreeMap::new();
        args.insert("year".to_string(), "2025".to_string());
        let out = replace_angle_placeholders("Copyright <year> <unknown>", &args);
        assert_eq!(out, "Copyright 2025 <unknown>");
    }

    #[test]
    fn renders_mit_with_auto_year_when_missing() {
        let mut args = BTreeMap::new();
        args.insert("copyright holders".to_string(), "Clay".to_string());
        let rendered = render_spdx_license("MIT", &args).unwrap();
        let year = current_year_string();
        assert!(rendered.text.contains(&year));
        assert!(rendered.text.contains("Clay"));
    }
}
