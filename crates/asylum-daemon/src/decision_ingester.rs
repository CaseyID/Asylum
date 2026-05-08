use serde::Deserialize;

pub const ASYLUM_DECISION_PROTOCOL: &str = "stdout-line-v1";
pub const ASYLUM_DECISION_PROTOCOL_MARKER: &str = "@@asylum:decision.request";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionProtocolRequest {
    pub text: String,
    pub actions: Vec<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StdoutDecisionIngestionEvent {
    OutputText(String),
    DecisionRequest(DecisionProtocolRequest),
}

#[derive(Default)]
pub struct StdoutDecisionLineIngestor {
    carry: String,
}

#[derive(Deserialize)]
struct DecisionProtocolPayload {
    text: String,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    source: Option<String>,
}

impl StdoutDecisionLineIngestor {
    pub fn ingest(&mut self, chunk: &str) -> Vec<StdoutDecisionIngestionEvent> {
        let mut events = Vec::new();
        self.carry.push_str(chunk);
        let buffered = std::mem::take(&mut self.carry);

        for raw_line in buffered.split_inclusive('\n') {
            if !raw_line.ends_with('\n') {
                self.carry.push_str(raw_line);
                continue;
            }

            let normalized_line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
            match parse_decision_line(normalized_line) {
                ParsedDecisionLine::Decision(request) => {
                    events.push(StdoutDecisionIngestionEvent::DecisionRequest(request))
                }
                ParsedDecisionLine::MalformedMarker => {}
                ParsedDecisionLine::OutputText => events.push(
                    StdoutDecisionIngestionEvent::OutputText(raw_line.to_string()),
                ),
            }
        }

        events
    }

    /// Emit any buffered carry bytes as OutputText without waiting for a newline.
    /// Use this after each raw read to avoid losing TUI output that never terminates
    /// with a newline (e.g. ANSI cursor-positioning sequences from full-screen TUIs).
    /// Only flushes if the carry does not look like a partial decision-protocol marker;
    /// in that case the bytes stay buffered until finalize.
    pub fn flush_partial(&mut self) -> Vec<StdoutDecisionIngestionEvent> {
        if self.carry.is_empty() {
            return Vec::new();
        }
        if self.carry.contains(ASYLUM_DECISION_PROTOCOL_MARKER) {
            // Might be a partial marker — leave in carry.
            return Vec::new();
        }
        let text = std::mem::take(&mut self.carry);
        vec![StdoutDecisionIngestionEvent::OutputText(text)]
    }

    pub fn finalize(&mut self) -> Vec<StdoutDecisionIngestionEvent> {
        let mut events = Vec::new();
        if self.carry.is_empty() {
            return events;
        }

        let remaining = std::mem::take(&mut self.carry);
        match parse_decision_line(&remaining) {
            ParsedDecisionLine::Decision(request) => {
                events.push(StdoutDecisionIngestionEvent::DecisionRequest(request))
            }
            ParsedDecisionLine::MalformedMarker => {}
            ParsedDecisionLine::OutputText => {
                events.push(StdoutDecisionIngestionEvent::OutputText(remaining))
            }
        }
        events
    }
}

enum ParsedDecisionLine {
    Decision(DecisionProtocolRequest),
    MalformedMarker,
    OutputText,
}

fn parse_decision_line(line: &str) -> ParsedDecisionLine {
    if !line.starts_with(ASYLUM_DECISION_PROTOCOL_MARKER) {
        return ParsedDecisionLine::OutputText;
    }

    let encoded = match line
        .get(ASYLUM_DECISION_PROTOCOL_MARKER.len()..)
        .map(str::trim)
    {
        Some(payload) if !payload.is_empty() => payload,
        _ => return ParsedDecisionLine::MalformedMarker,
    };

    if encoded == "{}" {
        return ParsedDecisionLine::MalformedMarker;
    }

    let payload: DecisionProtocolPayload = match serde_json::from_str(encoded) {
        Ok(payload) => payload,
        Err(_) => return ParsedDecisionLine::MalformedMarker,
    };

    if payload.text.trim().is_empty() {
        return ParsedDecisionLine::MalformedMarker;
    }

    ParsedDecisionLine::Decision(DecisionProtocolRequest {
        text: payload.text,
        actions: payload.actions,
        source: payload.source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_control_line_across_chunk_boundary() {
        let mut ingester = StdoutDecisionLineIngestor::default();

        let events =
            ingester.ingest(
                "hello\n@@asylum:decision.request {\"text\":\"Allow?\",\"actions\":[\"approve\",\"deny\"],",
            );
        assert_eq!(
            events,
            vec![StdoutDecisionIngestionEvent::OutputText(
                "hello\n".to_string()
            )]
        );

        let events = ingester.ingest("\"source\":\"permission_prompt\"}\nworld\n");
        assert_eq!(
            events,
            vec![
                StdoutDecisionIngestionEvent::DecisionRequest(DecisionProtocolRequest {
                    text: "Allow?".to_string(),
                    actions: vec!["approve".to_string(), "deny".to_string()],
                    source: Some("permission_prompt".to_string()),
                }),
                StdoutDecisionIngestionEvent::OutputText("world\n".to_string())
            ]
        );
        assert_eq!(
            ingester.finalize(),
            Vec::<StdoutDecisionIngestionEvent>::new()
        );
    }

    #[test]
    fn parser_ignores_malformed_marker_line() {
        let mut ingester = StdoutDecisionLineIngestor::default();
        let events = ingester.ingest("@@asylum:decision.request {\"text\": \"missing\":\n");
        assert_eq!(events, Vec::<StdoutDecisionIngestionEvent>::new());
        assert_eq!(
            ingester.finalize(),
            Vec::<StdoutDecisionIngestionEvent>::new()
        );
    }

    #[test]
    fn parser_does_not_treat_prose_as_control() {
        let mut ingester = StdoutDecisionLineIngestor::default();
        let events = ingester
            .ingest("plain @@asylum:decision.request {\"text\":\"nope\"} with trailing words\n");
        assert_eq!(
            events,
            vec![StdoutDecisionIngestionEvent::OutputText(
                "plain @@asylum:decision.request {\"text\":\"nope\"} with trailing words\n"
                    .to_string()
            )]
        );
        assert_eq!(
            ingester.finalize(),
            Vec::<StdoutDecisionIngestionEvent>::new()
        );
    }

    #[test]
    fn parser_ignores_unknown_fields_and_uses_required_text() {
        let mut ingester = StdoutDecisionLineIngestor::default();
        let events = ingester.ingest(
            "@@asylum:decision.request {\"text\":\"approve this?\",\"foo\":123,\"actions\":[\"approve\"]}\n",
        );
        assert_eq!(
            events,
            vec![StdoutDecisionIngestionEvent::DecisionRequest(
                DecisionProtocolRequest {
                    text: "approve this?".to_string(),
                    actions: vec!["approve".to_string()],
                    source: None,
                }
            )]
        );
    }

    #[test]
    fn flush_partial_emits_tui_bytes_without_newline() {
        let mut ingester = StdoutDecisionLineIngestor::default();
        // Simulate a TUI frame: ANSI escape sequences with no trailing newline.
        let events = ingester.ingest("\x1b[1;1H\x1b[0mwelcome");
        // ingest buffers the whole chunk because there is no newline.
        assert_eq!(events, Vec::<StdoutDecisionIngestionEvent>::new());
        // flush_partial drains carry as OutputText.
        let flushed = ingester.flush_partial();
        assert_eq!(
            flushed,
            vec![StdoutDecisionIngestionEvent::OutputText(
                "\x1b[1;1H\x1b[0mwelcome".to_string()
            )]
        );
        // carry is now empty; subsequent flush returns nothing.
        assert_eq!(
            ingester.flush_partial(),
            Vec::<StdoutDecisionIngestionEvent>::new()
        );
    }

    #[test]
    fn flush_partial_preserves_partial_decision_marker() {
        let mut ingester = StdoutDecisionLineIngestor::default();
        // Partial decision marker — must not be flushed prematurely.
        let events = ingester.ingest("@@asylum:decision.request {\"text\":\"half");
        assert_eq!(events, Vec::<StdoutDecisionIngestionEvent>::new());
        let flushed = ingester.flush_partial();
        // Marker is in carry, so flush_partial returns nothing.
        assert_eq!(flushed, Vec::<StdoutDecisionIngestionEvent>::new());
    }
}
