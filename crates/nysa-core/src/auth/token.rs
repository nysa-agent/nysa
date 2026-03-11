use base58::{FromBase58, ToBase58};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const TOKEN_PREFIX: &str = "nysa_";
const TOKEN_LENGTH: usize = 32;

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Invalid token format: {0}")]
    InvalidFormat(String),
    #[error("Token hashing failed: {0}")]
    HashingFailed(String),
    #[error("Token verification failed")]
    VerificationFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub prefix: String,
    pub data: Vec<u8>,
}

impl Token {
    pub fn generate() -> Self {
        let data: Vec<u8> = (0..TOKEN_LENGTH).map(|_| rand::random::<u8>()).collect();
        Self {
            prefix: TOKEN_PREFIX.to_string(),
            data,
        }
    }

    pub fn from_string(s: &str) -> Result<Self, TokenError> {
        if !s.starts_with(TOKEN_PREFIX) {
            return Err(TokenError::InvalidFormat(format!(
                "Token must start with '{}'",
                TOKEN_PREFIX
            )));
        }

        let rest = &s[TOKEN_PREFIX.len()..];
        let data = base58::FromBase58::from_base58(rest)
            .map_err(|_| TokenError::InvalidFormat("Invalid base58".to_string()))?;

        Ok(Self {
            prefix: TOKEN_PREFIX.to_string(),
            data,
        })
    }

    pub fn to_string(&self) -> String {
        format!("{}{}", self.prefix, self.data.to_base58())
    }

    pub fn hash(&self) -> Result<String, TokenError> {
        let mut hasher = Sha256::new();
        hasher.update(&self.data);
        let result = hasher.finalize();
        Ok(result.to_base58())
    }

    pub fn verify_hash(&self, hash: &str) -> bool {
        let Ok(own_hash) = self.hash() else {
            return false;
        };
        own_hash == hash
    }
}

pub fn generate_token() -> String {
    Token::generate().to_string()
}

pub fn hash_token(token: &str) -> Result<String, TokenError> {
    Token::from_string(token)?.hash()
}

pub fn verify_token(token: &str, hash: &str) -> bool {
    Token::from_string(token)
        .map(|t| t.verify_hash(hash))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token = Token::generate();
        assert!(token.to_string().starts_with("nysa_"));
        assert!(token.to_string().len() > TOKEN_PREFIX.len());
    }

    #[test]
    fn test_token_roundtrip() {
        let token = Token::generate();
        let serialized = token.to_string();
        let parsed = Token::from_string(&serialized).unwrap();
        assert_eq!(token.data, parsed.data);
    }

    #[test]
    fn test_token_hash_and_verify() {
        let token = Token::generate();
        let hash = token.hash().unwrap();
        assert!(token.verify_hash(&hash));
    }
}
