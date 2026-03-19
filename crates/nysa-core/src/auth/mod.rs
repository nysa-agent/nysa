pub mod linking;
pub mod provider;
pub mod rate_limit;
pub mod session;
pub mod token;

pub use linking::{LinkingCodeError, LinkingCodeService};
pub use provider::{AuthError, AuthProvider, AuthService, PlatformProfile};
pub use rate_limit::{RateLimitResult, RateLimiter};
pub use session::{Session, SessionError, SessionManager};
pub use token::{
    Token, TokenError, compute_lookup_hash, generate_linking_code, generate_token,
    hash_linking_code, hash_token, verify_linking_code, verify_token,
};
