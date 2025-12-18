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
            LicenseError::UnterminatedDirective { spdx } => write!(f, "unterminated SPDX template directive in {spdx} text"),
            LicenseError::MissingTemplateVar { spdx, name } => write!(f, "missing SPDX template variable {name:?} for {spdx}"),
        }
    }
}

impl std::error::Error for LicenseError {}

pub fn render_spdx_license(spdx: &str, template_args: &BTreeMap<String, String>) -> Result<RenderedLicense, LicenseError> {
    use std::str::FromStr;

    let parsed: &dyn license::License = <&dyn license::License>::from_str(spdx)
        .map_err(|_| LicenseError::UnknownSpdxId { spdx: spdx.to_string() })?;

    let raw = parsed.text();
    let expanded = expand_spdx_template(spdx, raw, template_args)?;
    let expanded = replace_angle_placeholders(&expanded, template_args);
    Ok(RenderedLicense { spdx: spdx.to_string(), text: expanded })
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
            return Err(LicenseError::UnterminatedDirective { spdx: spdx.to_string() });
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

        if directive.to_ascii_lowercase().starts_with("var;") || directive.eq_ignore_ascii_case("var") {
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
        let Some((k, v)) = part.split_once('=') else { continue };
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

    Err(LicenseError::MissingTemplateVar { spdx: spdx.to_string(), name })
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
}
