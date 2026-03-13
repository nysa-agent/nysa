use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base58::ToBase58;
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const TOKEN_PREFIX: &str = "nysa_";
const TOKEN_LENGTH: usize = 32;
const LINKING_CODE_LENGTH: usize = 8;
const LINKING_CODE_PREFIX: &str = "link_";

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Invalid token format: {0}")]
    InvalidFormat(String),
    #[error("Token hashing failed: {0}")]
    HashingFailed(String),
    #[error("Token verification failed")]
    VerificationFailed,
    #[error("Hash parsing failed: {0}")]
    HashParseError(String),
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
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);

        let password_hash = argon2
            .hash_password(&self.data, &salt)
            .map_err(|e| TokenError::HashingFailed(e.to_string()))?;

        Ok(password_hash.to_string())
    }

    pub fn verify_hash(&self, hash: &str) -> bool {
        let argon2 = Argon2::default();

        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(_) => return false,
        };

        argon2.verify_password(&self.data, &parsed_hash).is_ok()
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

/// Generate a shorter linking code for cross-platform auth
pub fn generate_linking_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();

    let code: String = (0..LINKING_CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    format!("{}{}", LINKING_CODE_PREFIX, code)
}

/// Hash a linking code using Argon2
pub fn hash_linking_code(code: &str) -> Result<String, TokenError> {
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);

    let password_hash = argon2
        .hash_password(code.as_bytes(), &salt)
        .map_err(|e| TokenError::HashingFailed(e.to_string()))?;

    Ok(password_hash.to_string())
}

/// Verify a linking code against its hash
pub fn verify_linking_code(code: &str, hash: &str) -> bool {
    let argon2 = Argon2::default();

    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    argon2
        .verify_password(code.as_bytes(), &parsed_hash)
        .is_ok()
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
        assert!(!token.verify_hash("invalid_hash"));
    }

    #[test]
    fn test_linking_code_generation() {
        let code = generate_linking_code();
        assert!(code.starts_with(LINKING_CODE_PREFIX));
        assert_eq!(code.len(), LINKING_CODE_PREFIX.len() + LINKING_CODE_LENGTH);
    }

    #[test]
    fn test_linking_code_hash_verify() {
        let code = generate_linking_code();
        let hash = hash_linking_code(&code).unwrap();
        assert!(verify_linking_code(&code, &hash));
        assert!(!verify_linking_code(&generate_linking_code(), &hash));
    }
}
