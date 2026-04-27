use std::fmt::Write as _;

use asylum_core::api::NativeAttachResponse;

pub fn render_native_attach_command(target: &NativeAttachResponse) -> String {
    let mut output = String::new();
    let _ = write!(output, "{}", target.command);

    for arg in &target.args {
        output.push(' ');
        if arg.contains(' ') {
            output.push('"');
            output.push_str(arg);
            output.push('"');
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
        .map(|(name, value)| format!("{name}={value}"))
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
            "ASYLUM_BASE_URL".to_string(),
            "http://127.0.0.1:7717".to_string(),
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
            "ASYLUM_BASE_URL=http://127.0.0.1:7717 asylum attach abc"
        );
    }
}
