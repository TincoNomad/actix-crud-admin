use crate::auth::{AuthError, JsonUserStore, UserStore};
use std::path::PathBuf;

/// Errors that can occur in CLI operations.
#[derive(Debug)]
pub enum CliError {
    Io(String),
    Auth(AuthError),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Io(msg) => write!(f, "{}", msg),
            CliError::Auth(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CliError {}

impl From<AuthError> for CliError {
    fn from(e: AuthError) -> Self {
        CliError::Auth(e)
    }
}

impl From<String> for CliError {
    fn from(e: String) -> Self {
        CliError::Io(e)
    }
}

/// Print help text for the admin CLI.
pub fn print_help(bin_name: &str) {
    println!("Actix Web Admin CLI");
    println!();
    println!("Usage: {} <command> [options]", bin_name);
    println!();
    println!("Commands:");
    println!("  createsuperuser | create | add   Create a new superuser (interactive)");
    println!("  deleteuser <username>             Delete a user by username");
    println!("  updatepassword <username>         Update a user's password (interactive)");
    println!("  listusers | list | ls             List all users");
    println!("  help                              Show this help");
    println!();
    println!("Options:");
    println!("  --file <path>    Path to users.json (default: users.json)");
    println!();
    println!("Examples:");
    println!("  {} createsuperuser", bin_name);
    println!("  {} createsuperuser --file /path/to/users.json", bin_name);
    println!("  {} deleteuser admin", bin_name);
    println!("  {} updatepassword admin", bin_name);
    println!("  {} listusers", bin_name);
}

/// Parse `--file <path>` from args, defaulting to `users.json`.
pub fn parse_file_arg(args: &[String]) -> PathBuf {
    if args.len() >= 2 && args[0] == "--file" {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("users.json")
    }
}

/// Interactive superuser creation using dialoguer prompts.
///
/// Must be called from within a tokio runtime (e.g. `#[tokio::main]`).
pub async fn create_superuser_interactive(args: &[String]) -> Result<(), CliError> {
    let file_path = parse_file_arg(args);

    println!("Create Superuser");
    println!("Users will be stored in: {:?}", file_path);
    println!();

    let username: String = dialoguer::Input::new()
        .with_prompt("Username")
        .interact_text()
        .map_err(|e| CliError::Io(format!("input error: {}", e)))?;

    let email: String = dialoguer::Input::new()
        .with_prompt("Email")
        .interact_text()
        .map_err(|e| CliError::Io(format!("input error: {}", e)))?;

    let name: String = dialoguer::Input::new()
        .with_prompt("Name")
        .interact_text()
        .map_err(|e| CliError::Io(format!("input error: {}", e)))?;

    let password: String = dialoguer::Password::new()
        .with_prompt("Password")
        .with_confirmation("Confirm password", "Passwords do not match")
        .interact()
        .map_err(|e| CliError::Io(format!("input error: {}", e)))?;

    let store = JsonUserStore::new(&file_path);

    store
        .create_user(&username, &email, &name, &password, true)
        .await
        .map(|user| {
            println!();
            println!(
                "Superuser '{}' created successfully! (id: {})",
                user.name, user.id
            );
        })
        .map_err(CliError::from)
}

/// Delete a user by username from a JsonUserStore at the given path.
///
/// Must be called from within a tokio runtime (e.g. `#[tokio::main]`).
pub async fn delete_user(args: &[String]) -> Result<(), CliError> {
    let (file_path, username) = parse_delete_args(args)?;
    let store = JsonUserStore::new(&file_path);
    store
        .delete_user(&username)
        .await
        .map(|_| println!("User '{}' deleted successfully.", username))
        .map_err(CliError::from)
}

/// List all users from a JsonUserStore at the given path.
///
/// Must be called from within a tokio runtime (e.g. `#[tokio::main]`).
pub async fn list_users(args: &[String]) -> Result<(), CliError> {
    let file_path = parse_file_arg(args);
    let store = JsonUserStore::new(&file_path);

    let users = store.all_users().map_err(CliError::from)?;
    if users.is_empty() {
        println!("No users found.");
        return Ok(());
    }
    println!("Users stored in: {:?}", file_path);
    println!();
    println!("{:<20} {:<30} {:<20} Superuser", "Username", "Email", "Name");
    println!("{}", "-".repeat(80));
    for u in &users {
        println!(
            "{:<20} {:<30} {:<20} {}",
            u.username,
            u.email,
            u.name,
            if u.is_superuser { "yes" } else { "no" }
        );
    }
    Ok(())
}

/// Update a user's password interactively from a JsonUserStore at the given path.
///
/// Must be called from within a tokio runtime (e.g. `#[tokio::main]`).
pub async fn update_password(args: &[String]) -> Result<(), CliError> {
    let (file_path, username) = parse_update_password_args(args)?;
    let store = JsonUserStore::new(&file_path);

    // Verify the user exists before prompting for a new password.
    store
        .find_by_username(&username)
        .await
        .map_err(CliError::from)?
        .ok_or(CliError::Auth(AuthError::NotFound))?;

    let password: String = dialoguer::Password::new()
        .with_prompt("New password")
        .with_confirmation("Confirm new password", "Passwords do not match")
        .interact()
        .map_err(|e| CliError::Io(format!("input error: {}", e)))?;

    store
        .update_password(&username, &password)
        .await
        .map(|_| println!("Password for user '{}' updated successfully.", username))
        .map_err(CliError::from)
}

pub(crate) fn parse_delete_args(args: &[String]) -> Result<(PathBuf, String), CliError> {
    if args.is_empty() {
        return Err(CliError::Io("Usage: admin-cli deleteuser <username> [--file <path>]".to_string()));
    }
    if args[0] == "--file" {
        let file_path = args.get(1).ok_or_else(|| {
            CliError::Io("--file requires a path argument".to_string())
        })?;
        let username = args.get(2).ok_or_else(|| {
            CliError::Io("Usage: admin-cli deleteuser <username> --file <path>".to_string())
        })?;
        Ok((PathBuf::from(file_path), username.clone()))
    } else {
        Ok((PathBuf::from("users.json"), args[0].clone()))
    }
}

pub(crate) fn parse_update_password_args(args: &[String]) -> Result<(PathBuf, String), CliError> {
    if args.is_empty() {
        return Err(CliError::Io(
            "Usage: admin-cli updatepassword <username> [--file <path>]".to_string(),
        ));
    }
    if args[0] == "--file" {
        let file_path = args.get(1).ok_or_else(|| {
            CliError::Io("--file requires a path argument".to_string())
        })?;
        let username = args.get(2).ok_or_else(|| {
            CliError::Io(
                "Usage: admin-cli updatepassword <username> --file <path>".to_string(),
            )
        })?;
        Ok((PathBuf::from(file_path), username.clone()))
    } else {
        Ok((PathBuf::from("users.json"), args[0].clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_delete_args_default_file() {
        let (path, username) = parse_delete_args(&["alice".to_string()]).unwrap();
        assert_eq!(path, PathBuf::from("users.json"));
        assert_eq!(username, "alice");
    }

    #[test]
    fn test_parse_delete_args_custom_file() {
        let (path, username) =
            parse_delete_args(&["--file".to_string(), "/tmp/u.json".to_string(), "bob".to_string()])
                .unwrap();
        assert_eq!(path, PathBuf::from("/tmp/u.json"));
        assert_eq!(username, "bob");
    }

    #[test]
    fn test_parse_delete_args_missing_username() {
        assert!(parse_delete_args(&[]).is_err());
    }

    #[test]
    fn test_parse_update_password_args_default_file() {
        let (path, username) = parse_update_password_args(&["alice".to_string()]).unwrap();
        assert_eq!(path, PathBuf::from("users.json"));
        assert_eq!(username, "alice");
    }

    #[test]
    fn test_parse_update_password_args_custom_file() {
        let (path, username) = parse_update_password_args(&[
            "--file".to_string(),
            "/tmp/u.json".to_string(),
            "bob".to_string(),
        ])
        .unwrap();
        assert_eq!(path, PathBuf::from("/tmp/u.json"));
        assert_eq!(username, "bob");
    }

    #[test]
    fn test_parse_update_password_args_missing_username() {
        assert!(parse_update_password_args(&[]).is_err());
    }
}
