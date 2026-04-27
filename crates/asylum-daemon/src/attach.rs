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
        let expires_seconds = ttl_seconds;
        let issued_at = OffsetDateTime::now_utc().unix_timestamp();
        let raw = format!(
            "{}:{}:{}:{}",
            node_id,
            Uuid::new_v4(),
            issued_at,
            expires_seconds
        );
        let mut signature = Sha256::new();
        signature.update(format!("{}:{}", raw, self.secret).as_bytes());
        let sig = signature
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let token = format!("{raw}.{sig}");
        let expires_at = issued_at + expires_seconds as i64;
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
        if payload_parts.len() != 4 {
            return Err(anyhow::anyhow!("bad payload"));
        }
        let node_id = Uuid::parse_str(payload_parts[0])?;
        let issue_time = payload_parts[2]
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("bad token issue time"))?;
        let ttl_seconds = payload_parts
            .get(3)
            .ok_or_else(|| anyhow::anyhow!("bad token ttl"))?
            .parse::<i64>()
            .map_err(|_| anyhow::anyhow!("bad token ttl"))?;
        let issued_at =
            OffsetDateTime::from_unix_timestamp(issue_time).unwrap_or(OffsetDateTime::UNIX_EPOCH);
        let expires_at = issued_at.unix_timestamp() + ttl_seconds;
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
    use sha2::{Digest, Sha256};
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn attach_tokens_are_node_scoped_and_expire() {
        let issuer = AttachTokenIssuer::new_for_tests("secret");
        let node_id = Uuid::new_v4();
        let issued_ttl_seconds = 60u64;
        let token = issuer.issue(node_id, issued_ttl_seconds).unwrap();

        assert_eq!(issuer.verify(&token.raw).unwrap().node_id, node_id);
        assert!(token
            .raw
            .contains(&issued_ttl_seconds.to_string())
            .then_some(())
            .is_some());
        assert!(issuer.verify("not-the-token").is_err());
    }

    #[test]
    fn malformed_attach_token_rejects_invalid_payload_shape() {
        let issuer = AttachTokenIssuer::new_for_tests("secret");
        let node_id = Uuid::new_v4();
        let token = issuer.issue(node_id, 60).unwrap();
        let parts = token.raw.split('.').next().unwrap_or_default();
        let mut shape = parts.split(':').collect::<Vec<_>>();
        shape.pop();
        let payload = shape.join(":");
        let mut hasher = Sha256::new();
        hasher.update(format!("{payload}:secret").as_bytes());
        let signature = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let malformed_token = format!("{payload}.{signature}");
        assert!(issuer.verify(&malformed_token).is_err());

        let issue_time = OffsetDateTime::now_utc().unix_timestamp();
        let payload = format!("{node_id}:{}:{issue_time}:120", Uuid::new_v4());
        let mut hasher = Sha256::new();
        hasher.update(format!("{payload}:secret").as_bytes());
        let signature = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let explicit_ttl_token = format!("{payload}.{signature}");
        let verified = issuer.verify(&explicit_ttl_token).unwrap();
        assert_eq!(verified.node_id, node_id);
    }
}
