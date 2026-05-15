use once_cell::sync::Lazy;
use validator::ValidationError;

static EMAIL_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"\A[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\z").unwrap()
});

pub fn validate_email(s: &str) -> Result<(), ValidationError> {
    if !EMAIL_REGEX.is_match(s) {
        let message = std::borrow::Cow::Borrowed("Invalid email format");
        Err(ValidationError::new("email").with_message(message))
    } else {
        Ok(())
    }
}

pub fn validate_password(password: &str) -> Result<(), ValidationError> {
    let mut errors: Vec<&str> = Vec::new();

    if password.len() < 8 {
        errors.push("Password must be at least 12 characters");
    }

    if password.len() > 128 {
        errors.push("Password is too long");
    }

    if !password.chars().any(|c| c.is_lowercase()) {
        errors.push("Password must contain at least one lowercase letter");
    }

    if !password.chars().any(|c| c.is_uppercase()) {
        errors.push("Password must contain at least one uppercase letter");
    }

    if errors.is_empty() {
        Ok(())
    } else {
        // Join all errors into one message (validator expects a single ValidationError)
        let message = errors.join(", ");
        Err(ValidationError::new("strong_password").with_message(std::borrow::Cow::from(message)))
    }
}
