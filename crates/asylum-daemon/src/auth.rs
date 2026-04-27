use anyhow::Result;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct IssuedOwnerToken {
    pub token_id: Uuid,
    pub raw_token: String,
    pub stored_hash: String,
    pub name: String,
    pub scope: Vec<String>,
    pub expires_at_epoch_secs: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthMode {
    Disabled,
    OwnerToken { expected_hashes: Vec<String> },
}

impl AuthMode {
    pub fn disabled() -> Self {
        Self::Disabled
    }
}

#[derive(Clone, Debug)]
pub enum AuthScope {
    Full,
}

pub fn issue_owner_token(
    name: &str,
    scope: &[String],
    ttl_seconds: Option<u64>,
) -> Result<IssuedOwnerToken> {
    let token_id = Uuid::new_v4();
    let raw = format!("asylum-owner-{}-{}", token_id, Uuid::new_v4());
    let stored_hash = hash_token(&raw);
    let ttl = ttl_seconds.unwrap_or(3600);
    let expires_at = chrono_like_now() + ttl as i64;
    Ok(IssuedOwnerToken {
        token_id,
        raw_token: raw,
        stored_hash,
        name: name.to_string(),
        scope: scope.to_vec(),
        expires_at_epoch_secs: expires_at,
    })
}

pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn verify_token(raw: &str, stored_hash: &str) -> bool {
    hash_token(raw) == stored_hash
}

#[derive(Clone, Debug)]
pub struct TokenVerification {
    pub token_id: Uuid,
    pub owner: String,
    pub scopes: Vec<String>,
}

pub fn validate_header(header: &str, mode: &AuthMode) -> Option<TokenVerification> {
    match mode {
        AuthMode::Disabled => Some(TokenVerification {
            token_id: Uuid::nil(),
            owner: "disabled".to_string(),
            scopes: vec!["*".to_string()],
        }),
        AuthMode::OwnerToken { expected_hashes } => {
            let token = header
                .strip_prefix("Bearer ")
                .or_else(|| header.strip_prefix("bearer "));
            token.and_then(|raw| {
                let hash = hash_token(raw);
                if expected_hashes.iter().any(|allowed| allowed == &hash) {
                    Some(TokenVerification {
                        token_id: Uuid::nil(),
                        owner: "owner".to_string(),
                        scopes: vec!["*".to_string()],
                    })
                } else {
                    None
                }
            })
        }
    }
}

pub fn chrono_like_now() -> i64 {
    use time::OffsetDateTime;
    let duration = OffsetDateTime::now_utc() - time::OffsetDateTime::UNIX_EPOCH;
    duration.whole_seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_verification_rejects_wrong_secret() {
        let issued = issue_owner_token("test-owner", &["node.list".to_string()], None).unwrap();
        assert!(verify_token(&issued.raw_token, &issued.stored_hash));
        assert!(!verify_token("wrong-token", &issued.stored_hash));
    }
}
