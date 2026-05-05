use std::fmt::Write as _;

use asylum_types::api::NativeAttachResponse;

/// Single-quote-escape a shell token so it is safe to use in a shell command line (L9).
/// Any single-quote in the value is replaced with `'\''` (end quote, literal ', reopen quote).
fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', r"'\''");
    format!("'{escaped}'")
}

/// Returns true if the string needs quoting (contains any shell-special character).
fn needs_quoting(s: &str) -> bool {
    s.chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
}

pub fn render_native_attach_command(target: &NativeAttachResponse) -> String {
    let mut output = String::new();
    let _ = write!(output, "{}", target.command);

    for arg in &target.args {
        output.push(' ');
        if needs_quoting(arg) {
            output.push_str(&shell_quote(arg));
        } else {
            output.push_str(arg);
        }
    }

    if target.environment.is_empty() {
        return output;
    }

    let env = target
        .environment
        .iter()
        .map(|(name, value)| {
            if needs_quoting(value) {
                format!("{name}={}", shell_quote(value))
            } else {
                format!("{name}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{env} {output}")
}

pub fn format_native_attach_prompt(target: &NativeAttachResponse) -> String {
    let command = render_native_attach_command(target);
    let mut lines = vec!["Native attach command:".to_string(), command];

    if !target.environment.is_empty() {
        lines.push(
            "Environment variables: ".to_string()
                + &target
                    .environment
                    .iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join(" "),
        );
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn render_native_command_includes_args_and_environment() {
        let mut env = BTreeMap::new();
        env.insert(
            "ASYLUM_SOCKET_PATH".to_string(),
            "/tmp/asylum/run/asylum.sock".to_string(),
        );

        let target = NativeAttachResponse {
            label: "Attach".to_string(),
            command: "asylum".to_string(),
            args: vec!["attach".to_string(), "abc".to_string()],
            environment: env,
        };

        let command = render_native_attach_command(&target);
        assert_eq!(
            command,
            "ASYLUM_SOCKET_PATH=/tmp/asylum/run/asylum.sock asylum attach abc"
        );
    }
}
