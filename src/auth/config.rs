//! Auth configuration: JWT secret and token lifetimes.

use std::time::Duration;

use rand::RngCore;

const ENV_SECRET: &str = "LEDGER_JWT_SECRET";

/// Static configuration for token issuance and validation.
#[derive(Clone)]
pub struct AuthConfig {
    pub(crate) secret: Vec<u8>,
    pub issuer: String,

    pub access_ttl: Duration,
    pub refresh_ttl: Duration,

    /// Multiplier applied to both TTLs when the `stay_logged_in` flag is set
    /// on login. 4x means 15m → 1h access, 7d → 28d refresh.
    pub stay_logged_in_multiplier: u32,
}

impl AuthConfig {
    /// Loads config from the environment, generating a random dev secret if
    /// `LEDGER_JWT_SECRET` is unset (logs a warning).
    pub fn from_env() -> Self {
        let secret = match std::env::var(ENV_SECRET) {
            Ok(s) if !s.is_empty() => s.into_bytes(),
            _ => {
                tracing::warn!(
                    "{ENV_SECRET} not set; generating an ephemeral secret. \
                     All tokens will be invalidated on restart. \
                     Set {ENV_SECRET} to a long random string in production."
                );
                let mut buf = [0u8; 64];
                rand::thread_rng().fill_bytes(&mut buf);
                buf.to_vec()
            }
        };

        Self {
            secret,
            issuer: "ledger".to_string(),
            access_ttl: Duration::from_secs(15 * 60),
            refresh_ttl: Duration::from_secs(7 * 24 * 60 * 60),
            stay_logged_in_multiplier: 4,
        }
    }

    pub fn access_ttl(&self, stay_logged_in: bool) -> Duration {
        if stay_logged_in {
            self.access_ttl * self.stay_logged_in_multiplier
        } else {
            self.access_ttl
        }
    }

    pub fn refresh_ttl(&self, stay_logged_in: bool) -> Duration {
        if stay_logged_in {
            self.refresh_ttl * self.stay_logged_in_multiplier
        } else {
            self.refresh_ttl
        }
    }

    pub fn secret(&self) -> &[u8] {
        &self.secret
    }
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthConfig")
            .field("secret", &"<redacted>")
            .field("issuer", &self.issuer)
            .field("access_ttl", &self.access_ttl)
            .field("refresh_ttl", &self.refresh_ttl)
            .field("stay_logged_in_multiplier", &self.stay_logged_in_multiplier)
            .finish()
    }
}
