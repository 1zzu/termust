//! The supported subset of an OpenSSH client config.
//!
//! This is deliberately small and honest: one literal alias per `Host` block,
//! the four directives Termust can use, and a note for everything else. The
//! parser reads no other file, expands nothing, resolves no name, opens no
//! socket and starts no process — [`parse`] is a pure function from text to a
//! preview, so it cannot write anything or reach the network by construction.
//!
//! Anything the parser does not understand is reported as a warning rather than
//! guessed at. A block it cannot use is still shown, marked with why.
//!
//! The rules, in one place:
//!
//! - The keyword is matched without regard to case. Leading whitespace and the
//!   whitespace between the keyword and its value may vary, and `Keyword=Value`
//!   is read the same as `Keyword Value`.
//! - An empty line is ignored, and a line whose first non-whitespace character
//!   is `#` is a comment. A trailing comment on a directive line is not a
//!   comment: OpenSSH does not support one either, and such a value is refused
//!   rather than trimmed behind the user's back.
//! - A `Host` block is used only when its pattern is exactly one literal alias.
//! - The alias becomes the label. Without `HostName`, it is also the address.
//! - Without `Port`, the port is 22. Without `User`, the block is invalid:
//!   Termust has no default username to fall back to.
//! - The first value of a directive wins; a repeat is reported as a note.

use super::model::{ConfigWarning, DEFAULT_PORT, EntryStatus, ImportEntry, ImportPreview};

/// A value read from a directive, with the line it came from.
struct Value {
    text: String,
    line: u32,
}

/// Why a `Host` pattern cannot be used.
enum PatternProblem {
    /// A pattern this build does not read: wildcards, negation, several aliases.
    Unsupported(String),
    /// A `Host` line that names nothing at all.
    Invalid(String),
}

/// One `Host` block while it is being read.
struct Block {
    line: u32,
    patterns: String,
    host_name: Option<Value>,
    user: Option<Value>,
    port: Option<Value>,
    /// Notes that do not block the entry: a directive that was skipped, or a
    /// repeated directive.
    notes: Vec<ConfigWarning>,
    /// Notes that do block it, so its status becomes `invalid`.
    problems: Vec<ConfigWarning>,
}

/// Parses the supported subset of an OpenSSH client config.
///
/// The same content always produces the same preview, including the entry
/// indexes: nothing here depends on the file system, the clock or the vault.
pub fn parse(content: &str) -> ImportPreview {
    let mut blocks: Vec<Block> = Vec::new();
    let mut warnings: Vec<ConfigWarning> = Vec::new();
    let mut current: Option<Block> = None;
    // Inside a Match section every directive belongs to a rule this build does
    // not read, so they are skipped without a second note for each one.
    let mut in_match = false;

    for (position, raw) in content.lines().enumerate() {
        let line = position as u32 + 1;
        let text = raw.trim();

        if text.is_empty() || text.starts_with('#') {
            continue;
        }

        if text.ends_with('\\') {
            let message = "Line continuation is not supported, so this line was ignored.";
            match current.as_mut() {
                Some(block) => block.notes.push(warning(Some(line), message)),
                None => warnings.push(warning(Some(line), message)),
            }
            continue;
        }

        let (token, value) = split_directive(text);
        let keyword = token.to_ascii_lowercase();

        if keyword == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            in_match = false;
            current = Some(Block::new(line, value));
            continue;
        }

        if keyword == "match" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            in_match = true;
            warnings.push(warning(
                Some(line),
                "Match sections are not supported, so every line until the next Host line was skipped.",
            ));
            continue;
        }

        if in_match {
            continue;
        }

        match current.as_mut() {
            Some(block) => block.apply(line, &keyword, token, value),
            None => warnings.push(warning(
                Some(line),
                format!(
                    "The \"{}\" directive is outside a Host block and was ignored.",
                    described(token)
                ),
            )),
        }
    }

    if let Some(block) = current.take() {
        blocks.push(block);
    }

    let entries = blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| finish(index, block))
        .collect();

    ImportPreview { entries, warnings }
}

/// Splits a line into its keyword and the rest, which is the value as written.
///
/// The keyword ends at the first whitespace or the first `=`, whichever comes
/// first, because OpenSSH accepts `Keyword Value` and `Keyword=Value` alike.
fn split_directive(text: &str) -> (&str, &str) {
    let end = text
        .find(|letter: char| letter.is_whitespace() || letter == '=')
        .unwrap_or(text.len());
    let rest = text[end..].trim_start();
    (
        text[..end].trim_end_matches('='),
        rest.trim_start_matches('=').trim_start(),
    )
}

impl Block {
    fn new(line: u32, patterns: &str) -> Self {
        Self {
            line,
            patterns: patterns.trim().to_string(),
            host_name: None,
            user: None,
            port: None,
            notes: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// Records one directive of this block. The keyword is lowercased for the
    /// match; `as_written` is what a message names, so the user sees the
    /// directive spelled the way their file spells it.
    fn apply(&mut self, line: u32, keyword: &str, as_written: &str, value: &str) {
        match keyword {
            "hostname" => set(
                &mut self.host_name,
                "HostName",
                line,
                value,
                &mut self.notes,
                &mut self.problems,
            ),
            "user" => set(
                &mut self.user,
                "User",
                line,
                value,
                &mut self.notes,
                &mut self.problems,
            ),
            "port" => set(
                &mut self.port,
                "Port",
                line,
                value,
                &mut self.notes,
                &mut self.problems,
            ),
            other => {
                let message = match skipped_message(other) {
                    Some(message) => message.to_string(),
                    None => format!(
                        "The \"{}\" directive is not supported and was ignored.",
                        described(as_written)
                    ),
                };
                self.notes.push(warning(Some(line), message));
            }
        }
    }
}

/// Stores the first value of a directive. A repeat keeps the first value and
/// says so, so a config is never rewritten by the second one.
fn set(
    slot: &mut Option<Value>,
    field: &str,
    line: u32,
    value: &str,
    notes: &mut Vec<ConfigWarning>,
    problems: &mut Vec<ConfigWarning>,
) {
    if slot.is_some() {
        notes.push(warning(
            Some(line),
            format!("{field} appears more than once in this block; the first value is used."),
        ));
        return;
    }

    let text = value.trim();
    if let Some(problem) = value_problem(text) {
        problems.push(warning(Some(line), format!("{field} {problem}.")));
    }
    *slot = Some(Value {
        text: text.to_string(),
        line,
    });
}

/// Why a value cannot be used, as the tail of a sentence. The value itself is
/// never repeated.
fn value_problem(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("has no value");
    }
    if value.starts_with('"') || value.starts_with('\'') {
        return Some("is quoted, and quoting is not interpreted");
    }
    if value.chars().any(char::is_whitespace) {
        return Some("is not a single value (a trailing comment must be on its own line)");
    }
    if value
        .chars()
        .any(|letter| matches!(letter, '%' | '$' | '`'))
    {
        return Some("uses token, environment or command expansion, which is not supported");
    }
    None
}

/// Why a `Host` pattern cannot be used by this build.
fn pattern_problem(patterns: &str) -> Option<PatternProblem> {
    if patterns.is_empty() {
        return Some(PatternProblem::Invalid(
            "The Host line names no alias.".to_string(),
        ));
    }

    let aliases: Vec<&str> = patterns.split_whitespace().collect();
    if aliases.len() > 1 {
        return Some(PatternProblem::Unsupported(
            "More than one alias on one Host line is not supported. Use one Host line per alias."
                .to_string(),
        ));
    }

    let alias = aliases[0];
    if alias.starts_with('!') {
        return Some(PatternProblem::Unsupported(
            "Negated Host patterns are not supported.".to_string(),
        ));
    }
    if alias.contains(['*', '?']) {
        return Some(PatternProblem::Unsupported(
            "Wildcard Host patterns are not supported. Name each host explicitly.".to_string(),
        ));
    }
    if value_problem(alias).is_some() {
        return Some(PatternProblem::Unsupported(
            "This Host pattern uses a value Termust cannot read.".to_string(),
        ));
    }
    None
}

/// The note for a directive this build recognises but does not apply.
///
/// Each message names what was skipped and what to do instead. No value of the
/// directive is repeated, and no file the directive points at is ever read.
/// `None` means the directive is not one this build knows at all.
fn skipped_message(keyword: &str) -> Option<&'static str> {
    let message = match keyword {
        "proxyjump" => "ProxyJump is not imported. Connect to the jump host on its own instead.",
        "proxycommand" => "ProxyCommand is not imported: Termust never runs a local command.",
        "localforward" => "LocalForward is not imported; port forwarding is not supported.",
        "remoteforward" => "RemoteForward is not imported; port forwarding is not supported.",
        "dynamicforward" => "DynamicForward is not imported; port forwarding is not supported.",
        "identityfile" => {
            "IdentityFile is not imported and the file is never read. Add a Credential to this host instead."
        }
        "certificatefile" => "CertificateFile is not imported and the file is never read.",
        "identityagent" => "IdentityAgent is not imported; there is no SSH agent support yet.",
        "preferredauthentications" => {
            "PreferredAuthentications is not imported. Termust authenticates with the Credential linked to the host."
        }
        "include" => "Include is not followed and the referenced file is never read.",
        "forwardagent" => "ForwardAgent is not imported; there is no SSH agent support yet.",
        _ => return None,
    };
    Some(message)
}

/// A directive name as it is safe to show: the first word of a line, kept
/// short and printable. It is a name, never a value.
fn described(keyword: &str) -> String {
    keyword
        .chars()
        .take(32)
        .map(|letter| {
            if letter.is_ascii_graphic() {
                letter
            } else {
                '?'
            }
        })
        .collect()
}

/// Turns a finished block into the entry the preview shows.
fn finish(index: usize, block: Block) -> ImportEntry {
    let mut problems = block.problems;
    let alias = block.patterns.split_whitespace().next().unwrap_or("");

    // The Host line first: a pattern this build cannot read is the reason to
    // skip the block, whatever the directives inside it said.
    let pattern = pattern_problem(&block.patterns);
    if let Some(problem) = &pattern {
        let message = match problem {
            PatternProblem::Unsupported(message) | PatternProblem::Invalid(message) => {
                message.clone()
            }
        };
        problems.insert(0, warning(Some(block.line), message));
    }

    // An empty HostName is already reported as having no value; the alias is
    // what the address falls back to.
    let address = match block.host_name.as_ref() {
        Some(value) if !value.text.is_empty() => value.text.clone(),
        _ => alias.to_string(),
    };

    let username = block
        .user
        .as_ref()
        .map(|value| value.text.clone())
        .unwrap_or_default();
    if block.user.is_none() {
        problems.push(warning(
            Some(block.line),
            "User is required: Termust has no default username to fall back to.",
        ));
    }

    let port = match block.port.as_ref() {
        // An empty Port is already reported as having no value.
        Some(value) if value.text.is_empty() => None,
        Some(value) => match parse_port(&value.text) {
            Ok(port) => Some(port),
            Err(message) => {
                problems.push(warning(Some(value.line), message));
                None
            }
        },
        None => Some(DEFAULT_PORT),
    };

    let invalid_pattern = matches!(pattern, Some(PatternProblem::Invalid(_)));
    let status = if pattern.is_some() && !invalid_pattern {
        EntryStatus::Unsupported
    } else if !problems.is_empty() {
        EntryStatus::Invalid
    } else {
        EntryStatus::Ready
    };

    problems.extend(block.notes);

    ImportEntry {
        index,
        label: alias.to_string(),
        address,
        port,
        username,
        status,
        warnings: problems,
    }
}

/// A port a TCP connection can actually use.
fn parse_port(value: &str) -> Result<u16, String> {
    match value.parse::<i64>() {
        Ok(port) if (1..=65535).contains(&port) => Ok(port as u16),
        Ok(_) => Err("Port must be between 1 and 65535.".to_string()),
        Err(_) => Err("Port must be a number between 1 and 65535.".to_string()),
    }
}

fn warning(line: Option<u32>, message: impl Into<String>) -> ConfigWarning {
    ConfigWarning {
        line,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openssh::model::{DEFAULT_PORT, EntryStatus};

    /// A config that is exactly what this build supports.
    const SUPPORTED: &str = "\
# Production boxes
Host production
    HostName 192.0.2.10
    User deploy
    Port 22

Host staging
\tHostName=192.0.2.20
\tUser deploy
";

    fn statuses(preview: &ImportPreview) -> Vec<EntryStatus> {
        preview.entries.iter().map(|entry| entry.status).collect()
    }

    fn messages(entry: &ImportEntry) -> Vec<String> {
        entry
            .warnings
            .iter()
            .map(|warning| warning.message.clone())
            .collect()
    }

    #[test]
    fn one_supported_block_is_ready() {
        let preview = parse("Host production\n  HostName 192.0.2.10\n  User deploy\n  Port 2222\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        assert_eq!(preview.warnings, vec![]);

        let entry = &preview.entries[0];
        assert_eq!(entry.index, 0);
        assert_eq!(entry.label, "production");
        assert_eq!(entry.address, "192.0.2.10");
        assert_eq!(entry.port, Some(2222));
        assert_eq!(entry.username, "deploy");
        assert_eq!(entry.warnings, vec![]);
    }

    #[test]
    fn several_blocks_keep_their_order_and_indexes() {
        let preview = parse(SUPPORTED);

        assert_eq!(
            preview
                .entries
                .iter()
                .map(|entry| (entry.index, entry.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "production"), (1, "staging")]
        );
        assert_eq!(
            statuses(&preview),
            vec![EntryStatus::Ready, EntryStatus::Ready]
        );
        // A blank line and a comment change nothing.
        assert_eq!(preview.warnings, vec![]);
    }

    #[test]
    fn the_alias_is_the_label_and_the_default_address() {
        let preview = parse("Host db1\n  User root\n");
        let entry = &preview.entries[0];

        assert_eq!(entry.label, "db1");
        assert_eq!(entry.address, "db1", "the alias is the address");
        assert_eq!(entry.port, Some(DEFAULT_PORT));
        assert_eq!(entry.status, EntryStatus::Ready);
    }

    #[test]
    fn the_default_port_is_22_and_a_declared_one_is_used() {
        let preview = parse("Host a\n  User u\n\nHost b\n  User u\n  Port 2200\n");

        assert_eq!(preview.entries[0].port, Some(22));
        assert_eq!(preview.entries[1].port, Some(2200));
    }

    #[test]
    fn keywords_are_case_insensitive() {
        for config in [
            "HOST x\n  hostname a.example.com\n  USER u\n  PORT 22\n",
            "host x\n  HOSTNAME a.example.com\n  user u\n  port 22\n",
            "Host x\n  HoStNaMe a.example.com\n  UsEr u\n  PoRt 22\n",
        ] {
            let preview = parse(config);
            assert_eq!(statuses(&preview), vec![EntryStatus::Ready], "{config}");
            assert_eq!(preview.entries[0].address, "a.example.com");
            assert_eq!(preview.entries[0].username, "u");
            assert_eq!(preview.entries[0].port, Some(22));
        }
    }

    /// OpenSSH accepts `Keyword Value` and `Keyword=Value`; so does this.
    #[test]
    fn a_keyword_may_be_separated_from_its_value_by_an_equals_sign() {
        for config in [
            "Host=prod\nHostName=192.0.2.10\nUser=deploy\nPort=2200\n",
            "Host =prod\nHostName= 192.0.2.10\nUser =deploy\nPort =2200\n",
            "Host= prod\nHostName = 192.0.2.10\nUser= deploy\nPort = 2200\n",
        ] {
            let preview = parse(config);
            assert_eq!(statuses(&preview), vec![EntryStatus::Ready], "{config}");
            let entry = &preview.entries[0];
            assert_eq!(entry.label, "prod", "{config}");
            assert_eq!(entry.address, "192.0.2.10", "{config}");
            assert_eq!(entry.username, "deploy", "{config}");
            assert_eq!(entry.port, Some(2200), "{config}");
        }

        // An `=` inside a value belongs to the value, as it does in OpenSSH.
        let preview = parse("Host prod=1\n  User u\n  HostName 192.0.2.10\n");
        assert_eq!(preview.entries[0].label, "prod=1");
        assert_eq!(preview.entries[0].status, EntryStatus::Ready);
    }

    #[test]
    fn whitespace_comments_and_blank_lines_are_tolerated() {
        let config = "\
# a comment
   # an indented comment

\tHost    spaced
\t\tHostName\t\tspaced.example.com
   User      deploy

Host      second
        User      deploy
";

        let preview = parse(config);
        assert_eq!(
            statuses(&preview),
            vec![EntryStatus::Ready, EntryStatus::Ready]
        );
        assert_eq!(preview.entries[0].label, "spaced");
        assert_eq!(preview.entries[0].address, "spaced.example.com");
        assert_eq!(preview.entries[0].username, "deploy");
        assert_eq!(preview.entries[1].label, "second");
        assert_eq!(preview.entries[1].address, "second");
        assert!(
            preview
                .warnings
                .iter()
                .all(|warning| warning.line == Some(1) || warning.line == Some(2)),
            "only the commented line may be mentioned: {:?}",
            preview.warnings
        );
    }

    #[test]
    fn a_block_without_a_user_is_invalid() {
        let preview = parse("Host production\n  HostName 192.0.2.10\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Invalid]);
        let entry = &preview.entries[0];
        assert_eq!(entry.address, "192.0.2.10");
        assert_eq!(entry.username, "");
        assert!(
            messages(entry)
                .iter()
                .any(|message| message.contains("User is required")),
            "{:?}",
            entry.warnings
        );
    }

    #[test]
    fn an_unusable_port_is_invalid() {
        for (value, expected) in [
            ("0", "Port must be between 1 and 65535."),
            ("65536", "Port must be between 1 and 65535."),
            ("99999", "Port must be between 1 and 65535."),
            ("abc", "Port must be a number between 1 and 65535."),
            ("22.5", "Port must be a number between 1 and 65535."),
        ] {
            let preview = parse(&format!("Host x\n  User u\n  Port {value}\n"));

            assert_eq!(
                statuses(&preview),
                vec![EntryStatus::Invalid],
                "port {value}"
            );
            let entry = &preview.entries[0];
            assert_eq!(entry.port, None, "an unusable port must not be defaulted");
            assert!(
                messages(entry).contains(&expected.to_string()),
                "port {value} produced {:?}",
                entry.warnings
            );
        }
    }

    #[test]
    fn an_empty_value_on_any_supported_directive_is_invalid() {
        for config in [
            "Host x\n  User\n",
            "Host x\n  User u\n  Port\n",
            "Host x\n  User u\n  HostName\n",
        ] {
            let preview = parse(config);
            assert_eq!(statuses(&preview), vec![EntryStatus::Invalid], "{config}");
        }

        // HostName with no value falls back to the alias, but the empty
        // directive is still reported rather than silently ignored.
        let preview = parse("Host x\n  User u\n  HostName\n");
        assert_eq!(preview.entries[0].address, "x");
    }

    #[test]
    fn a_host_line_without_an_alias_is_invalid() {
        let preview = parse("Host\n  User u\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Invalid]);
        assert_eq!(preview.entries[0].label, "");
        assert!(
            messages(&preview.entries[0])
                .iter()
                .any(|message| message.contains("names no alias"))
        );
    }

    #[test]
    fn wildcards_negation_and_several_aliases_are_unsupported() {
        for config in [
            "Host *\n  User u\n",
            "Host prod-*\n  User u\n",
            "Host pro?uction\n  User u\n",
            "Host !internal\n  User u\n",
            "Host prod staging\n  User u\n",
        ] {
            let preview = parse(config);
            assert_eq!(
                statuses(&preview),
                vec![EntryStatus::Unsupported],
                "{config} produced {:?}",
                preview.entries
            );
            assert_eq!(
                preview.entries[0].index, 0,
                "an unsupported block still has its own row"
            );
        }
    }

    #[test]
    fn a_host_star_default_is_not_applied_to_anything() {
        let preview = parse(
            "Host *\n  User root\n  Port 2222\n\nHost prod\n  HostName 192.0.2.10\n  User deploy\n",
        );

        assert_eq!(
            statuses(&preview),
            vec![EntryStatus::Unsupported, EntryStatus::Ready]
        );
        // The literal host keeps its own values: no precedence is emulated.
        assert_eq!(preview.entries[1].username, "deploy");
        assert_eq!(preview.entries[1].port, Some(22));
    }

    #[test]
    fn include_is_never_followed_and_is_reported() {
        let preview = parse("Host prod\n  User deploy\n  Include other.conf\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        let entry = &preview.entries[0];
        assert!(
            messages(entry)
                .iter()
                .any(|message| message.contains("Include is not followed")),
            "{:?}",
            entry.warnings
        );
        // Two entries only if the included file were read; it never is.
        assert_eq!(preview.entries.len(), 1);
    }

    #[test]
    fn identity_file_is_reported_and_never_read() {
        let preview = parse("Host prod\n  User deploy\n  IdentityFile ~/.ssh/id_ed25519\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        let entry = &preview.entries[0];
        assert!(
            messages(entry)
                .iter()
                .any(|message| message.contains("IdentityFile is not imported")),
            "{:?}",
            entry.warnings
        );
        // The path is not repeated anywhere, and no credential is described.
        assert!(
            !messages(entry)
                .iter()
                .any(|message| message.contains("id_ed25519")),
            "the key path must not be repeated: {:?}",
            entry.warnings
        );
    }

    #[test]
    fn a_material_connection_override_is_warned_about() {
        let preview =
            parse("Host prod\n  User deploy\n  ProxyJump bastion\n  ProxyCommand nc host 22\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        let warnings = messages(&preview.entries[0]);
        assert!(warnings.iter().any(|message| message.contains("ProxyJump")));
        assert!(
            warnings
                .iter()
                .any(|message| message.contains("never runs a local command")),
            "ProxyCommand must say that nothing is executed: {warnings:?}"
        );
        assert!(
            !warnings
                .iter()
                .any(|message| message.contains("nc host 22")),
            "the command must not be repeated: {warnings:?}"
        );
    }

    #[test]
    fn forwarding_directives_are_reported_as_skipped() {
        let preview = parse(
            "Host prod\n  User deploy\n  LocalForward 8080 localhost:80\n  RemoteForward 9090 localhost:90\n  DynamicForward 1080\n",
        );

        let warnings = messages(&preview.entries[0]);
        for name in ["LocalForward", "RemoteForward", "DynamicForward"] {
            assert!(
                warnings.iter().any(|message| message.contains(name)),
                "{name} was not reported: {warnings:?}"
            );
        }
    }

    #[test]
    fn a_match_section_is_not_read_as_hosts() {
        let preview = parse(
            "Host prod\n  User deploy\n\nMatch host *.internal\n  User root\n  Port 2222\n\nHost real\n  User deploy\n",
        );

        assert_eq!(
            preview
                .entries
                .iter()
                .map(|entry| entry.label.as_str())
                .collect::<Vec<_>>(),
            vec!["prod", "real"]
        );
        assert_eq!(
            statuses(&preview),
            vec![EntryStatus::Ready, EntryStatus::Ready]
        );
        // Nothing from the Match section leaks into either host.
        for entry in &preview.entries {
            assert_eq!(entry.port, Some(22));
        }
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.message.contains("Match sections are not supported")),
            "{:?}",
            preview.warnings
        );
    }

    #[test]
    fn expansion_tokens_are_refused_in_every_supported_field() {
        for config in [
            "Host x\n  User %r\n",
            "Host x\n  User u\n  HostName %h.example.com\n",
            "Host x\n  User u\n  HostName $(whoami)\n",
            "Host x\n  User u\n  HostName `whoami`\n",
            "Host x\n  User u\n  HostName $HOME\n",
            "Host %h\n  User u\n",
        ] {
            let preview = parse(config);
            assert_ne!(
                statuses(&preview),
                vec![EntryStatus::Ready],
                "{config} was accepted"
            );
        }
    }

    #[test]
    fn a_value_with_a_trailing_comment_is_refused_rather_than_trimmed() {
        let preview = parse("Host prod\n  User deploy\n  HostName 192.0.2.10 # production\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Invalid]);
        let entry = &preview.entries[0];
        assert_eq!(
            entry.address, "192.0.2.10 # production",
            "the value is shown as written"
        );
        assert!(
            messages(entry)
                .iter()
                .any(|message| message.contains("must be on its own line")),
            "{:?}",
            entry.warnings
        );
    }

    #[test]
    fn quoted_values_are_refused_rather_than_unquoted() {
        let preview = parse("Host prod\n  User \"deploy\"\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Invalid]);
        assert!(
            messages(&preview.entries[0])
                .iter()
                .any(|message| message.contains("quoting is not interpreted"))
        );
    }

    #[test]
    fn line_continuation_is_reported_and_ignored() {
        let preview = parse("Host prod\\\n  User deploy\n");

        // The continued Host line never starts a block, so there is nothing
        // importable and the reason is in the file-level notes.
        assert_eq!(preview.entries.len(), 0);
        assert!(
            preview
                .warnings
                .iter()
                .any(|warning| warning.message.contains("Line continuation")),
            "{:?}",
            preview.warnings
        );
    }

    #[test]
    fn a_repeated_directive_keeps_the_first_value_and_says_so() {
        let preview = parse("Host prod\n  User first\n  User second\n  Port 2200\n  Port 2222\n");
        let entry = &preview.entries[0];

        assert_eq!(entry.username, "first");
        assert_eq!(entry.port, Some(2200));
        assert_eq!(
            entry.status,
            EntryStatus::Ready,
            "a repeat is a note, not a failure"
        );
        let warnings = messages(entry);
        assert!(
            warnings
                .iter()
                .filter(|message| message.contains("more than once"))
                .count()
                >= 2
        );
    }

    #[test]
    fn a_directive_outside_a_host_block_is_reported() {
        let preview = parse("User root\nPort 2222\n\nHost prod\n  User deploy\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        assert_eq!(
            preview.entries[0].username, "deploy",
            "a global default is not applied"
        );
        assert_eq!(preview.entries[0].port, Some(22));
        let lines: Vec<Option<u32>> = preview
            .warnings
            .iter()
            .map(|warning| warning.line)
            .collect();
        assert_eq!(lines, vec![Some(1), Some(2)]);
    }

    #[test]
    fn an_unknown_directive_is_named_without_its_value() {
        let preview = parse("Host prod\n  User deploy\n  FancySecret hunter2\n");

        assert_eq!(statuses(&preview), vec![EntryStatus::Ready]);
        let warnings = messages(&preview.entries[0]);
        assert!(
            warnings
                .iter()
                .any(|message| message.contains("FancySecret")),
            "{warnings:?}"
        );
        assert!(
            !warnings.iter().any(|message| message.contains("hunter2")),
            "a value must never be repeated: {warnings:?}"
        );
    }

    #[test]
    fn warnings_carry_the_line_they_are_about() {
        let preview = parse("Host prod\n  User deploy\n  UnknownThing here\n  Port 99999\n");

        let entry = &preview.entries[0];
        let unknown = entry
            .warnings
            .iter()
            .find(|warning| warning.message.contains("UnknownThing"))
            .expect("the unknown directive was reported");
        assert_eq!(unknown.line, Some(3));

        let port = entry
            .warnings
            .iter()
            .find(|warning| warning.message.contains("Port must be"))
            .expect("the bad port was reported");
        assert_eq!(port.line, Some(4));
    }

    #[test]
    fn an_entry_keeps_being_reported_when_it_is_incomplete() {
        // A block that is missing its user is still shown, with the reason.
        let preview = parse("Host incomplete\n  Port 2222\n");

        assert_eq!(preview.entries.len(), 1);
        assert_eq!(preview.entries[0].status, EntryStatus::Invalid);
        assert_eq!(preview.entries[0].port, Some(2222));
    }

    #[test]
    fn the_parser_is_deterministic() {
        let preview = parse(SUPPORTED);
        let again = parse(SUPPORTED);

        assert_eq!(preview, again);
        assert_eq!(
            preview
                .entries
                .iter()
                .map(|entry| entry.index)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn an_empty_or_trivial_input_produces_no_entries() {
        for config in ["", "\n\n", "# only a comment\n", "   \t  \n"] {
            let preview = parse(config);
            assert_eq!(preview.entries, vec![], "{config:?}");
            assert_eq!(preview.warnings, vec![], "{config:?}");
        }
    }

    #[test]
    fn a_unicode_alias_and_address_survive_intact() {
        let preview = parse("Host pröd\n  HostName 192.0.2.10\n  User de ploy\n");

        assert_eq!(preview.entries[0].label, "pröd");
        assert_eq!(preview.entries[0].address, "192.0.2.10");
        assert_eq!(
            preview.entries[0].status,
            EntryStatus::Invalid,
            "a username with a space is refused"
        );
    }

    /// A tiny deterministic generator, so the "fuzz" run is repeatable and adds
    /// no dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut state = self.0;
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            self.0 = state;
            state.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        fn below(&mut self, limit: u64) -> u64 {
            self.next() % limit.max(1)
        }
    }

    /// Arbitrary input must never panic the parser, and must always produce the
    /// same preview twice.
    #[test]
    fn arbitrary_input_never_panics_and_stays_deterministic() {
        const ALPHABET: [char; 24] = [
            'H', 'o', 's', 't', ' ', '\t', '\n', '\r', '#', '"', '\'', '\\', '%', '$', '`', '*',
            '!', '?', ':', '.', '0', '9', 'é', '\u{0}',
        ];

        let mut rng = Rng(0x1234_5678_9abc_def0);
        for round in 0..2_000 {
            let length = rng.below(200) as usize;
            let mut content = String::with_capacity(length);
            for _ in 0..length {
                content.push(ALPHABET[rng.below(ALPHABET.len() as u64) as usize]);
            }

            let preview = parse(&content);
            assert_eq!(
                preview,
                parse(&content),
                "round {round} was not deterministic"
            );
        }

        // Random unicode scalar values, so the parser also sees text no config
        // would ever contain.
        for round in 0..500 {
            let length = rng.below(80) as usize;
            let mut content = String::new();
            for _ in 0..length {
                if let Some(letter) = char::from_u32(rng.next() as u32) {
                    content.push(letter);
                }
            }
            assert_eq!(
                parse(&content),
                parse(&content),
                "round {round} was not deterministic"
            );
        }
    }

    /// A long but well-formed config is parsed in one pass; this is the shape a
    /// 1 MiB file has.
    #[test]
    fn a_large_config_is_parsed_without_incident() {
        let mut content = String::new();
        for index in 0..20_000 {
            content.push_str(&format!(
                "Host host-{index}\n  HostName 192.0.2.10\n  User deploy\n  Port 22\n\n"
            ));
        }

        let preview = parse(&content);
        assert_eq!(preview.entries.len(), 20_000);
        assert_eq!(preview.entries[19_999].label, "host-19999");
        assert_eq!(preview.entries[19_999].index, 19_999);
        assert!(preview.warnings.is_empty());
    }
}
