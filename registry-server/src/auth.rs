use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;

pub struct Auth {
    users: RwLock<HashMap<String, User>>,
    tokens: RwLock<HashMap<String, TokenData>>,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth").finish()
    }
}

#[derive(Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    password_hash: String,
    pub created_at: DateTime<Utc>,
}

struct TokenData {
    user_id: String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

static SECRET_KEY: Lazy<[u8; 32]> = Lazy::new(|| {
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    key
});

impl Auth {
    pub fn new() -> Self {
        Self {
            users: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, username: &str, email: &str, password: &str) -> Result<User, String> {
        let mut users = self.users.write();

        if users.contains_key(username) {
            return Err("Username already exists".to_string());
        }

        let password_hash =
            hash(password, DEFAULT_COST).map_err(|e| format!("Failed to hash password: {}", e))?;

        let user = User {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
            email: email.to_string(),
            password_hash,
            created_at: Utc::now(),
        };

        users.insert(username.to_string(), user.clone());
        Ok(user)
    }

    pub fn authenticate(&self, username: &str, password: &str) -> Result<User, String> {
        let users = self.users.read();
        let user = users.get(username).ok_or("Invalid credentials")?;

        let valid = verify(password, &user.password_hash)
            .map_err(|e| format!("Failed to verify password: {}", e))?;

        if !valid {
            return Err("Invalid credentials".to_string());
        }

        Ok(user.clone())
    }

    pub fn create_token(
        &self,
        user_id: &str,
        expires_in_days: Option<u32>,
    ) -> Result<String, String> {
        let token = generate_token_string();
        let expires_at =
            expires_in_days.map(|days| Utc::now() + chrono::Duration::days(days as i64));

        let mut tokens = self.tokens.write();
        tokens.insert(
            token.clone(),
            TokenData {
                user_id: user_id.to_string(),
                expires_at,
                created_at: Utc::now(),
            },
        );

        Ok(token)
    }

    pub fn validate_token(&self, token: &str) -> Option<User> {
        let tokens = self.tokens.read();
        let token_data = tokens.get(token)?;

        if let Some(expires_at) = token_data.expires_at {
            if expires_at < Utc::now() {
                return None;
            }
        }

        let users = self.users.read();
        users.values().find(|u| u.id == token_data.user_id).cloned()
    }

    pub fn revoke_token(&self, token: &str) -> bool {
        let mut tokens = self.tokens.write();
        tokens.remove(token).is_some()
    }
}

fn generate_token_string() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    hex::encode(bytes)
}

use uuid::Uuid;
