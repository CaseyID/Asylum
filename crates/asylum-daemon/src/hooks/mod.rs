use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use asylum_core::api::{HookAction, HookEventCatalogEntry, HookFiringRecord, HookRule};
use serde_json::Value as JsonValue;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::storage::{HookFiringRow, HookRow};

#[derive(Clone, Debug)]
pub struct HookEvent {
    pub event: String,
    pub node_id: Option<Uuid>,
    pub payload: JsonValue,
}

#[derive(Clone)]
pub struct HookEngine {
    sender: broadcast::Sender<HookEvent>,
}

impl HookEngine {
    pub fn new() -> Arc<Self> {
        let (sender, _rx) = broadcast::channel(256);
        Arc::new(Self { sender })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.sender.subscribe()
    }

    pub fn post(&self, event: HookEvent) {
        let _ = self.sender.send(event);
    }
}

impl Default for HookEngine {
    fn default() -> Self {
        Self {
            sender: broadcast::channel(256).0,
        }
    }
}

pub fn rule_from_row(row: HookRow) -> HookRule {
    let actions: Vec<HookAction> =
        serde_json::from_str(&row.actions_json).unwrap_or_else(|_| Vec::new());
    HookRule {
        id: row.id,
        name: row.name,
        enabled: row.enabled,
        event: row.event,
        filter: row.filter,
        actions,
        future: row.future,
        created_at_epoch_secs: row.created_at,
        updated_at_epoch_secs: row.updated_at,
    }
}

pub fn firing_record_from_row(row: HookFiringRow) -> HookFiringRecord {
    let payload = serde_json::from_str(&row.payload_json).unwrap_or(JsonValue::Null);
    HookFiringRecord {
        id: row.id,
        hook_id: row.hook_id,
        ts_epoch_secs: row.ts,
        trigger: row.trigger,
        outcome: row.outcome,
        ok: row.ok,
        payload,
    }
}

pub fn event_catalog() -> Vec<HookEventCatalogEntry> {
    static ENTRIES: &[(&str, &str)] = &[
        ("node.permission_requested", "Node requested human input"),
        ("node.exited", "Node exited"),
        ("node.errored", "Node errored"),
        ("node.idle", "Node idle"),
        ("node.ctx_pressure", "Node context pressure"),
        ("node.tool_call", "Node tool call"),
        ("graph.spawn", "Node spawned"),
        ("substrate.unreachable", "Substrate unreachable"),
        ("channel.inbound", "Inbound channel message"),
        ("schedule.5m", "Every five minutes"),
        ("schedule.30m", "Every thirty minutes"),
        ("schedule.cron", "Custom cron schedule"),
    ];
    ENTRIES
        .iter()
        .map(|(id, label)| HookEventCatalogEntry {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
}

pub fn evaluate_filter(filter: &str, payload: &JsonValue) -> bool {
    let trimmed = filter.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("any") {
        return true;
    }
    match parse_filter(trimmed) {
        Ok(expr) => expr.evaluate(payload),
        Err(err) => {
            tracing::warn!(filter = %trimmed, error = %err, "hook filter parse failed — event blocked (fail-closed)");
            false
        }
    }
}

#[derive(Debug)]
enum FilterExpr {
    Compare {
        key: String,
        op: CompareOp,
        value: String,
    },
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
}

#[derive(Debug, Clone, Copy)]
enum CompareOp {
    Eq,
    Ne,
    Ge,
    Le,
    Gt,
    Lt,
    Substring,
}

impl FilterExpr {
    fn evaluate(&self, payload: &JsonValue) -> bool {
        match self {
            FilterExpr::Compare { key, op, value } => evaluate_compare(payload, key, *op, value),
            FilterExpr::And(a, b) => a.evaluate(payload) && b.evaluate(payload),
            FilterExpr::Or(a, b) => a.evaluate(payload) || b.evaluate(payload),
        }
    }
}

fn evaluate_compare(payload: &JsonValue, key: &str, op: CompareOp, value: &str) -> bool {
    let actual = lookup_path(payload, key);
    let actual_str = match &actual {
        Some(JsonValue::String(s)) => s.clone(),
        Some(JsonValue::Number(n)) => n.to_string(),
        Some(JsonValue::Bool(b)) => b.to_string(),
        Some(JsonValue::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    match op {
        CompareOp::Eq => actual_str == value,
        CompareOp::Ne => actual_str != value,
        CompareOp::Substring => actual_str.contains(value),
        CompareOp::Ge | CompareOp::Le | CompareOp::Gt | CompareOp::Lt => {
            let lhs = actual_str.parse::<f64>().ok();
            let rhs = value.parse::<f64>().ok();
            match (lhs, rhs) {
                (Some(l), Some(r)) => match op {
                    CompareOp::Ge => l >= r,
                    CompareOp::Le => l <= r,
                    CompareOp::Gt => l > r,
                    CompareOp::Lt => l < r,
                    _ => false,
                },
                _ => false,
            }
        }
    }
}

fn lookup_path<'a>(payload: &'a JsonValue, key: &str) -> Option<JsonValue> {
    let mut current = payload.clone();
    for segment in key.split('.') {
        let next = match &current {
            JsonValue::Object(map) => map.get(segment).cloned(),
            _ => None,
        };
        match next {
            Some(value) => current = value,
            None => return None,
        }
    }
    Some(current)
}

fn parse_filter(input: &str) -> Result<FilterExpr> {
    let tokens = tokenize(input);
    let mut cursor = 0;
    let expr = parse_or(&tokens, &mut cursor)?;
    if cursor != tokens.len() {
        return Err(anyhow::anyhow!("unexpected trailing tokens"));
    }
    Ok(expr)
}

#[derive(Debug, Clone)]
enum Token {
    And,
    Or,
    Atom(String, CompareOp, String),
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut out = Vec::new();
    for raw_clause in split_top_level(input) {
        let clause = raw_clause.trim();
        if clause.is_empty() {
            continue;
        }
        if clause == "&&" {
            out.push(Token::And);
            continue;
        }
        if clause == "||" {
            out.push(Token::Or);
            continue;
        }
        if let Some(atom) = parse_atom(clause) {
            out.push(atom);
        }
    }
    out
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len()
            && (bytes[i] == b'&' && bytes[i + 1] == b'&'
                || bytes[i] == b'|' && bytes[i + 1] == b'|')
        {
            if !buf.trim().is_empty() {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            out.push(if bytes[i] == b'&' {
                "&&".to_string()
            } else {
                "||".to_string()
            });
            i += 2;
            continue;
        }
        buf.push(bytes[i] as char);
        i += 1;
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn parse_atom(clause: &str) -> Option<Token> {
    let ops = [
        ("==", CompareOp::Eq),
        ("!=", CompareOp::Ne),
        (">=", CompareOp::Ge),
        ("<=", CompareOp::Le),
        (">", CompareOp::Gt),
        ("<", CompareOp::Lt),
        ("~", CompareOp::Substring),
    ];
    for (sym, op) in ops {
        if let Some(idx) = clause.find(sym) {
            let key = clause[..idx].trim().to_string();
            let value = clause[idx + sym.len()..]
                .trim()
                .trim_matches('"')
                .to_string();
            if key.is_empty() {
                return None;
            }
            return Some(Token::Atom(key, op, value));
        }
    }
    None
}

fn parse_or(tokens: &[Token], cursor: &mut usize) -> Result<FilterExpr> {
    let mut left = parse_and(tokens, cursor)?;
    while *cursor < tokens.len() {
        match &tokens[*cursor] {
            Token::Or => {
                *cursor += 1;
                let right = parse_and(tokens, cursor)?;
                left = FilterExpr::Or(Box::new(left), Box::new(right));
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_and(tokens: &[Token], cursor: &mut usize) -> Result<FilterExpr> {
    let mut left = parse_atom_token(tokens, cursor)?;
    while *cursor < tokens.len() {
        match &tokens[*cursor] {
            Token::And => {
                *cursor += 1;
                let right = parse_atom_token(tokens, cursor)?;
                left = FilterExpr::And(Box::new(left), Box::new(right));
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_atom_token(tokens: &[Token], cursor: &mut usize) -> Result<FilterExpr> {
    if *cursor >= tokens.len() {
        return Err(anyhow::anyhow!("unexpected end of filter"));
    }
    let token = tokens[*cursor].clone();
    *cursor += 1;
    match token {
        Token::Atom(key, op, value) => Ok(FilterExpr::Compare { key, op, value }),
        _ => Err(anyhow::anyhow!("expected atom, got operator")),
    }
}

pub const SCHEDULE_5M: Duration = Duration::from_secs(300);
pub const SCHEDULE_30M: Duration = Duration::from_secs(1800);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_filter_matches() {
        let payload = serde_json::json!({});
        assert!(evaluate_filter("any", &payload));
        assert!(evaluate_filter("", &payload));
    }

    #[test]
    fn equality_and_dot_path() {
        let payload = serde_json::json!({"role": "worker", "node": {"id": "abc"}});
        assert!(evaluate_filter("role == worker", &payload));
        assert!(evaluate_filter("node.id == abc", &payload));
        assert!(!evaluate_filter("role == supervisor", &payload));
    }

    #[test]
    fn substring_and_compound() {
        let payload = serde_json::json!({"reason": "context window getting tight"});
        assert!(evaluate_filter("reason ~ context", &payload));
        assert!(evaluate_filter(
            "reason ~ context && reason ~ tight",
            &payload
        ));
        assert!(evaluate_filter(
            "reason == none || reason ~ tight",
            &payload
        ));
    }

    #[test]
    fn unparseable_filter_falls_back_to_any() {
        let payload = serde_json::json!({});
        assert!(evaluate_filter("?(?)*", &payload));
    }
}
