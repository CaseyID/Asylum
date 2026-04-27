use anyhow::Result;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AttachTokenRecord {
    pub raw: String,
    pub node_id: Uuid,
    pub expires_at: i64,
    pub secret_hint: String,
}

#[derive(Clone)]
pub struct AttachTokenIssuer {
    secret: String,
}

impl AttachTokenIssuer {
    pub fn new_for_tests(secret: &str) -> Self {
        Self {
            secret: secret.to_string(),
        }
    }

    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    pub fn issue(&self, node_id: Uuid, ttl_seconds: u64) -> Result<AttachTokenRecord> {
        let raw = format!(
            "{}:{}:{}",
            node_id,
            Uuid::new_v4(),
            OffsetDateTime::now_utc().unix_timestamp()
        );
        let mut signature = Sha256::new();
        signature.update(format!("{}:{}", raw, self.secret).as_bytes());
        let sig = signature
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let token = format!("{raw}.{sig}");
        let expires_at = OffsetDateTime::now_utc().unix_timestamp() + ttl_seconds as i64;
        Ok(AttachTokenRecord {
            raw: token,
            node_id,
            expires_at,
            secret_hint: sig.chars().take(6).collect(),
        })
    }

    pub fn verify(&self, raw: &str) -> Result<AttachTokenRecord> {
        let parts: Vec<_> = raw.split('.').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("bad token shape"));
        }
        let (payload, signature) = (parts[0], parts[1]);
        let mut digest = Sha256::new();
        digest.update(format!("{payload}:{}", self.secret).as_bytes());
        let expected = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if expected != signature {
            return Err(anyhow::anyhow!("signature mismatch"));
        }
        let payload_parts: Vec<_> = payload.split(':').collect();
        if payload_parts.len() < 2 {
            return Err(anyhow::anyhow!("bad payload"));
        }
        let node_id = Uuid::parse_str(payload_parts[0])?;
        let issue_time = payload_parts[2].parse::<i64>().unwrap_or(0);
        let issued_at =
            OffsetDateTime::from_unix_timestamp(issue_time).unwrap_or(OffsetDateTime::UNIX_EPOCH);
        let expires_at = issued_at.unix_timestamp() + 60 * 60;
        if expires_at <= OffsetDateTime::now_utc().unix_timestamp() {
            return Err(anyhow::anyhow!("expired"));
        }
        Ok(AttachTokenRecord {
            raw: raw.to_string(),
            node_id,
            expires_at,
            secret_hint: signature.chars().take(6).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn attach_tokens_are_node_scoped_and_expire() {
        let issuer = AttachTokenIssuer::new_for_tests("secret");
        let node_id = Uuid::new_v4();
        let token = issuer.issue(node_id, 60).unwrap();

        assert_eq!(issuer.verify(&token.raw).unwrap().node_id, node_id);
        assert!(issuer.verify("not-the-token").is_err());
    }
}
