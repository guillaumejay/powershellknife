use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PredictionSource {
    #[default]
    None,
    History,
    Plugin,
    HistoryAndPlugin,
}

impl PredictionSource {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::History,
        Self::Plugin,
        Self::HistoryAndPlugin,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::History => "History",
            Self::Plugin => "Plugin",
            Self::HistoryAndPlugin => "HistoryAndPlugin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "history" => Some(Self::History),
            "plugin" => Some(Self::Plugin),
            "historyandplugin" => Some(Self::HistoryAndPlugin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    #[default]
    Windows,
    Emacs,
    Vi,
}

impl EditMode {
    pub const ALL: &'static [Self] = &[Self::Windows, Self::Emacs, Self::Vi];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Emacs => "Emacs",
            Self::Vi => "Vi",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "windows" => Some(Self::Windows),
            "emacs" => Some(Self::Emacs),
            "vi" => Some(Self::Vi),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BellStyle {
    None,
    Visual,
    #[default]
    Audible,
}

impl BellStyle {
    pub const ALL: &'static [Self] = &[Self::None, Self::Visual, Self::Audible];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Visual => "Visual",
            Self::Audible => "Audible",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "visual" => Some(Self::Visual),
            "audible" => Some(Self::Audible),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PSReadLineSettings {
    pub history_no_duplicates: bool,
    pub history_search_cursor_moves_to_end: bool,
    pub prediction_source: PredictionSource,
    pub edit_mode: EditMode,
    pub bell_style: BellStyle,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub psreadline: PSReadLineSettings,
    pub modules: Vec<String>,
    pub aliases: BTreeMap<String, String>,
}

impl Settings {
    pub fn parse(inner_lines: &[String]) -> Self {
        let mut out = Settings::default();
        for line in inner_lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(rest) = strip_cmdlet(trimmed, "Set-PSReadLineOption") {
                parse_psreadline_option(rest, &mut out.psreadline);
            } else if let Some(rest) = strip_cmdlet(trimmed, "Import-Module") {
                let name = rest.trim();
                if !name.is_empty() && !out.modules.iter().any(|m| m == name) {
                    out.modules.push(name.to_string());
                }
            } else if let Some(rest) = strip_cmdlet(trimmed, "Set-Alias")
                && let Some((name, value)) = parse_alias(rest)
            {
                out.aliases.insert(name, value);
            }
        }
        out
    }

    pub fn serialize(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "Set-PSReadLineOption -HistoryNoDuplicates:${}",
            bool_lit(self.psreadline.history_no_duplicates)
        ));
        lines.push(format!(
            "Set-PSReadLineOption -HistorySearchCursorMovesToEnd:${}",
            bool_lit(self.psreadline.history_search_cursor_moves_to_end)
        ));
        lines.push(format!(
            "Set-PSReadLineOption -PredictionSource {}",
            self.psreadline.prediction_source.as_str()
        ));
        lines.push(format!(
            "Set-PSReadLineOption -EditMode {}",
            self.psreadline.edit_mode.as_str()
        ));
        lines.push(format!(
            "Set-PSReadLineOption -BellStyle {}",
            self.psreadline.bell_style.as_str()
        ));
        for m in &self.modules {
            lines.push(format!("Import-Module {m}"));
        }
        for (name, value) in &self.aliases {
            lines.push(format!(
                "Set-Alias {name} '{}'",
                escape_single_quotes(value)
            ));
        }
        lines
    }
}

fn strip_cmdlet<'a>(line: &'a str, cmdlet: &str) -> Option<&'a str> {
    let lower_line = line.to_ascii_lowercase();
    let lower_cmd = cmdlet.to_ascii_lowercase();
    let rest = lower_line.strip_prefix(&lower_cmd)?;
    let next_char_idx = cmdlet.len();
    let next = line.as_bytes().get(next_char_idx).copied();
    match next {
        None => Some(""),
        Some(b) if (b as char).is_whitespace() => Some(&line[next_char_idx..]),
        _ => {
            // Not a full cmdlet token (e.g. "Set-AliasX"); ignore.
            let _ = rest;
            None
        }
    }
}

fn parse_psreadline_option(s: &str, out: &mut PSReadLineSettings) {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if let Some(rest) = token.strip_prefix('-') {
            let (name, inline_val) = match rest.split_once(':') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            };
            match name.to_ascii_lowercase().as_str() {
                "historynoduplicates" => {
                    out.history_no_duplicates =
                        inline_val.and_then(parse_bool_literal).unwrap_or(true);
                }
                "historysearchcursormovestoend" => {
                    out.history_search_cursor_moves_to_end =
                        inline_val.and_then(parse_bool_literal).unwrap_or(true);
                }
                "predictionsource" => {
                    if let Some(v) = inline_val.or_else(|| tokens.get(i + 1).copied()) {
                        if let Some(ps) = PredictionSource::parse(v) {
                            out.prediction_source = ps;
                        }
                        if inline_val.is_none() {
                            i += 1;
                        }
                    }
                }
                "editmode" => {
                    if let Some(v) = inline_val.or_else(|| tokens.get(i + 1).copied()) {
                        if let Some(em) = EditMode::parse(v) {
                            out.edit_mode = em;
                        }
                        if inline_val.is_none() {
                            i += 1;
                        }
                    }
                }
                "bellstyle" => {
                    if let Some(v) = inline_val.or_else(|| tokens.get(i + 1).copied()) {
                        if let Some(bs) = BellStyle::parse(v) {
                            out.bell_style = bs;
                        }
                        if inline_val.is_none() {
                            i += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
}

fn parse_alias(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for c in s.chars() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut name: Option<String> = None;
    let mut value: Option<String> = None;
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.to_ascii_lowercase().as_str() {
            "-name" => {
                if let Some(v) = tokens.get(i + 1) {
                    name = Some(unquote(v));
                    i += 2;
                    continue;
                }
            }
            "-value" => {
                if let Some(v) = tokens.get(i + 1) {
                    value = Some(unquote(v));
                    i += 2;
                    continue;
                }
            }
            _ => {
                if name.is_none() {
                    name = Some(unquote(tok));
                } else if value.is_none() {
                    value = Some(unquote(tok));
                }
            }
        }
        i += 1;
    }

    match (name, value) {
        (Some(n), Some(v)) if !n.is_empty() => Some((n, v)),
        _ => None,
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[s.len() - 1];
        if first == last && (first == b'\'' || first == b'"') {
            let inner = &s[1..s.len() - 1];
            if first == b'\'' {
                return inner.replace("''", "'");
            }
            return inner.to_string();
        }
    }
    s.to_string()
}

fn parse_bool_literal(s: &str) -> Option<bool> {
    match s.to_ascii_lowercase().as_str() {
        "$true" | "true" => Some(true),
        "$false" | "false" => Some(false),
        _ => None,
    }
}

fn bool_lit(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_lines(settings: &Settings) -> Vec<String> {
        settings.serialize()
    }

    #[test]
    fn default_settings_serialize_deterministically() {
        let s = Settings::default();
        let lines = s.serialize();
        assert_eq!(
            lines,
            vec![
                "Set-PSReadLineOption -HistoryNoDuplicates:$false".to_string(),
                "Set-PSReadLineOption -HistorySearchCursorMovesToEnd:$false".to_string(),
                "Set-PSReadLineOption -PredictionSource None".to_string(),
                "Set-PSReadLineOption -EditMode Windows".to_string(),
                "Set-PSReadLineOption -BellStyle Audible".to_string(),
            ]
        );
    }

    #[test]
    fn parse_bool_option_without_value_means_true() {
        let lines = vec!["Set-PSReadLineOption -HistoryNoDuplicates".to_string()];
        let s = Settings::parse(&lines);
        assert!(s.psreadline.history_no_duplicates);
    }

    #[test]
    fn parse_bool_option_with_explicit_false() {
        let lines = vec!["Set-PSReadLineOption -HistoryNoDuplicates:$false".to_string()];
        let s = Settings::parse(&lines);
        assert!(!s.psreadline.history_no_duplicates);
    }

    #[test]
    fn parse_enum_option_positional() {
        let lines = vec!["Set-PSReadLineOption -PredictionSource History".to_string()];
        let s = Settings::parse(&lines);
        assert_eq!(s.psreadline.prediction_source, PredictionSource::History);
    }

    #[test]
    fn parse_enum_option_inline_value() {
        let lines = vec!["Set-PSReadLineOption -EditMode:Vi".to_string()];
        let s = Settings::parse(&lines);
        assert_eq!(s.psreadline.edit_mode, EditMode::Vi);
    }

    #[test]
    fn parse_bellstyle_visual() {
        let lines = vec!["Set-PSReadLineOption -BellStyle Visual".to_string()];
        let s = Settings::parse(&lines);
        assert_eq!(s.psreadline.bell_style, BellStyle::Visual);
    }

    #[test]
    fn parse_import_module() {
        let lines = vec![
            "Import-Module posh-git".to_string(),
            "Import-Module Terminal-Icons".to_string(),
        ];
        let s = Settings::parse(&lines);
        assert_eq!(s.modules, vec!["posh-git", "Terminal-Icons"]);
    }

    #[test]
    fn parse_import_module_dedup() {
        let lines = vec![
            "Import-Module posh-git".to_string(),
            "Import-Module posh-git".to_string(),
        ];
        let s = Settings::parse(&lines);
        assert_eq!(s.modules, vec!["posh-git"]);
    }

    #[test]
    fn parse_alias_positional_with_single_quoted_value() {
        let lines = vec!["Set-Alias ll 'Get-ChildItem -Force'".to_string()];
        let s = Settings::parse(&lines);
        assert_eq!(
            s.aliases.get("ll").map(String::as_str),
            Some("Get-ChildItem -Force")
        );
    }

    #[test]
    fn parse_alias_named_parameters() {
        let lines = vec!["Set-Alias -Name ll -Value 'Get-ChildItem -Force'".to_string()];
        let s = Settings::parse(&lines);
        assert_eq!(
            s.aliases.get("ll").map(String::as_str),
            Some("Get-ChildItem -Force")
        );
    }

    #[test]
    fn parse_ignores_comments_and_blank_lines() {
        let lines = vec![
            "".to_string(),
            "   ".to_string(),
            "# a comment".to_string(),
            "Import-Module Foo".to_string(),
        ];
        let s = Settings::parse(&lines);
        assert_eq!(s.modules, vec!["Foo"]);
    }

    #[test]
    fn parse_ignores_unknown_cmdlets() {
        let lines = vec![
            "Write-Host 'hello'".to_string(),
            "Import-Module Foo".to_string(),
        ];
        let s = Settings::parse(&lines);
        assert_eq!(s.modules, vec!["Foo"]);
    }

    #[test]
    fn strip_cmdlet_requires_whole_token() {
        let lines = vec!["Set-AliasOther foo bar".to_string()];
        let s = Settings::parse(&lines);
        assert!(s.aliases.is_empty());
    }

    #[test]
    fn roundtrip_is_stable_for_canonical_input() {
        let canonical = vec![
            "Set-PSReadLineOption -HistoryNoDuplicates:$true".to_string(),
            "Set-PSReadLineOption -HistorySearchCursorMovesToEnd:$false".to_string(),
            "Set-PSReadLineOption -PredictionSource History".to_string(),
            "Set-PSReadLineOption -EditMode Windows".to_string(),
            "Set-PSReadLineOption -BellStyle Visual".to_string(),
            "Import-Module posh-git".to_string(),
            "Import-Module Terminal-Icons".to_string(),
            "Set-Alias ll 'Get-ChildItem -Force'".to_string(),
        ];
        let parsed = Settings::parse(&canonical);
        let out = canonical_lines(&parsed);
        assert_eq!(out, canonical);
    }

    #[test]
    fn roundtrip_double_parse_is_stable() {
        let mut s = Settings::default();
        s.psreadline.history_no_duplicates = true;
        s.psreadline.prediction_source = PredictionSource::History;
        s.modules.push("posh-git".to_string());
        s.aliases
            .insert("ll".to_string(), "Get-ChildItem".to_string());

        let lines1 = s.serialize();
        let parsed = Settings::parse(&lines1);
        assert_eq!(parsed, s);
        let lines2 = parsed.serialize();
        assert_eq!(lines1, lines2);
    }

    #[test]
    fn alias_with_single_quote_in_value_is_escaped_and_reparsed() {
        let mut s = Settings::default();
        s.aliases
            .insert("weird".to_string(), "it's fine".to_string());
        let lines = s.serialize();
        assert!(lines.iter().any(|l| l.contains("'it''s fine'")));
        let parsed = Settings::parse(&lines);
        assert_eq!(
            parsed.aliases.get("weird").map(String::as_str),
            Some("it's fine")
        );
    }

    #[test]
    fn aliases_serialize_alphabetically() {
        let mut s = Settings::default();
        s.aliases.insert("b".to_string(), "2".to_string());
        s.aliases.insert("a".to_string(), "1".to_string());
        let lines = s.serialize();
        let alias_lines: Vec<&String> = lines
            .iter()
            .filter(|l| l.starts_with("Set-Alias"))
            .collect();
        assert_eq!(alias_lines.len(), 2);
        assert!(alias_lines[0].contains(" a "));
        assert!(alias_lines[1].contains(" b "));
    }
}
