use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The result type used throughout the shift system
type ShiftResult<T> = Result<T, ShiftError>;

/// Error type for shift operations
#[derive(Debug)]
enum ShiftError {
    IoError(io::Error),
    CommandFailed(String),
    ValidationFailed(String),
    AlreadyExists(String),
    NotFound(String),
    Custom(String),
}

impl From<io::Error> for ShiftError {
    fn from(error: io::Error) -> Self {
        ShiftError::IoError(error)
    }
}

/// The core trait that all shift operations must implement
trait Shift {
    /// Apply the shift to move to the next state
    fn apply(&self) -> ShiftResult<()>;

    /// Revert the shift to return to the previous state (if possible)
    fn revert(&self) -> ShiftResult<()>;

    /// Check if the shift has already been applied
    fn is_applied(&self) -> ShiftResult<bool>;

    /// Get a human-readable description of the shift
    fn describe(&self) -> String;
}

/// A shift that creates a directory if it doesn't exist
struct CreateDir {
    path: String,
}

impl CreateDir {
    fn new(path: String) -> Self {
        CreateDir { path }
    }
}

impl Shift for CreateDir {
    fn apply(&self) -> ShiftResult<()> {
        let path = Path::new(&self.path);

        if path.exists() {
            if path.is_dir() {
                return Ok(());
            } else {
                return Err(ShiftError::AlreadyExists(format!(
                    "Path {} exists but is not a directory",
                    path.display()
                )));
            }
        }

        fs::create_dir(path)?;
        Ok(())
    }

    fn revert(&self) -> ShiftResult<()> {
        let path = Path::new(&self.path);

        if !path.exists() {
            return Ok(());
        }

        if !path.is_dir() {
            return Err(ShiftError::ValidationFailed(format!(
                "Path {} is not a directory",
                path.display()
            )));
        }

        fs::remove_dir_all(path)?;

        Ok(())
    }

    fn is_applied(&self) -> ShiftResult<bool> {
        let path = Path::new(&self.path);
        Ok(path.exists() && path.is_dir())
    }

    fn describe(&self) -> String {
        format!("Create directory at {}", self.path)
    }
}

/// A shift that creates a file with specific content
struct CreateFile {
    path: String,
    content: String,
}

impl CreateFile {
    fn new(path: String, content: String) -> Self {
        CreateFile { path, content }
    }
}

impl Shift for CreateFile {
    fn apply(&self) -> ShiftResult<()> {
        let path = Path::new(&self.path);

        if path.exists() {
            return Err(ShiftError::AlreadyExists(format!(
                "File {} already exists",
                path.display()
            )));
        }

        fs::write(path, &self.content)?;
        Ok(())
    }

    fn revert(&self) -> ShiftResult<()> {
        let path = Path::new(&self.path);

        if !path.exists() {
            return Ok(());
        }

        if !path.is_file() {
            return Err(ShiftError::ValidationFailed(format!(
                "Path {} is not a file",
                path.display()
            )));
        }

        fs::remove_file(path)?;
        Ok(())
    }

    fn is_applied(&self) -> ShiftResult<bool> {
        let path = Path::new(&self.path);
        Ok(path.exists() && path.is_file())
    }

    fn describe(&self) -> String {
        format!("Create file at {} with specific content", self.path)
    }
}

/// A shift that executes a command
struct Cmd {
    command: String,
    args: Vec<String>,
    working_dir: Option<String>,
    success_exit_codes: Vec<i32>,
}

impl Cmd {
    fn new(
        command: String,
        args: Vec<String>,
        working_dir: Option<String>,
        success_exit_codes: Option<Vec<i32>>,
    ) -> Self {
        Cmd {
            command,
            args,
            working_dir,
            success_exit_codes: success_exit_codes.unwrap_or_else(|| vec![0]),
        }
    }
}

impl Shift for Cmd {
    fn apply(&self) -> ShiftResult<()> {
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);

        if let Some(dir) = &self.working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output()?;

        if !self
            .success_exit_codes
            .contains(&(output.status.code().unwrap_or(-1)))
        {
            return Err(ShiftError::CommandFailed(format!(
                "Command '{}' failed with exit code {:?}. Stderr: {}",
                self.command,
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    fn revert(&self) -> ShiftResult<()> {
        // Commands typically can't be reverted automatically
        Err(ShiftError::Custom(
            "Cannot automatically revert a command execution".to_string(),
        ))
    }

    fn is_applied(&self) -> ShiftResult<bool> {
        // Command execution doesn't have a persistent state to check
        Err(ShiftError::Custom(
            "Cannot check if a command has been applied".to_string(),
        ))
    }

    fn describe(&self) -> String {
        format!(
            "Execute command '{}' with args {:?}{}",
            self.command,
            self.args,
            self.working_dir
                .as_ref()
                .map_or(String::new(), |dir| format!(" in directory {}", dir))
        )
    }
}

/// A shift plan that represents a transition from one state to another
struct ShiftPlan {
    name: String,
    description: String,
    shifts: Vec<Box<dyn Shift>>,
}

impl ShiftPlan {
    fn new(name: String, description: String, shifts: Vec<Box<dyn Shift>>) -> Self {
        ShiftPlan {
            name,
            description,
            shifts,
        }
    }

    fn apply(&self) -> ShiftResult<()> {
        println!("Applying shift plan: {}", self.name);
        println!("Description: {}", self.description);

        let mut applied = Vec::new();

        for shift in &self.shifts {
            println!("Applying: {}", shift.describe());

            match shift.apply() {
                Ok(_) => {
                    println!("✓ Applied successfully");
                    applied.push(shift);
                }
                Err(e) => {
                    println!("✗ Failed to apply: {:#?}", e);

                    println!("Reverting already applied shifts...");
                    for applied_shift in applied.iter().rev() {
                        println!("Reverting: {}", applied_shift.describe());
                        match applied_shift.revert() {
                            Ok(_) => println!("✓ Reverted successfully"),
                            Err(e) => println!("✗ Failed to revert: {:#?}", e),
                        }
                    }

                    return Err(e);
                }
            }
        }

        println!("Shift plan applied successfully");
        Ok(())
    }

    fn revert(&self) -> ShiftResult<()> {
        println!("Reverting shift plan: {}", self.name);

        let mut errors = Vec::new();

        // Revert in reverse order
        for shift in self.shifts.iter().rev() {
            println!("Reverting: {}", shift.describe());

            match shift.revert() {
                Ok(_) => println!("✓ Reverted successfully"),
                Err(e) => {
                    println!("✗ Failed to revert: {:#?}", e);
                    errors.push(format!("{}: {:#?}", shift.describe(), e));
                }
            }
        }

        if !errors.is_empty() {
            return Err(ShiftError::Custom(format!(
                "Failed to revert some shifts: {}",
                errors.join(", ")
            )));
        }

        println!("Shift plan reverted successfully");
        Ok(())
    }
}

struct GitHubClone {
    repo_url: String,
    target_dir: String,
    branch: Option<String>,
    depth: Option<usize>,
    auth_token: Option<String>,
}

impl GitHubClone {
    fn new(
        repo_url: String,
        target_dir: String,
        branch: Option<String>,
        depth: Option<usize>,
        auth_token: Option<String>,
    ) -> Self {
        GitHubClone {
            repo_url,
            target_dir,
            branch,
            depth,
            auth_token,
        }
    }

    fn get_repo_name(&self) -> String {
        // Extract repository name from URL
        let parts: Vec<&str> = self.repo_url.split('/').collect();
        if parts.len() >= 1 {
            let repo_with_git = parts.last().unwrap();
            if repo_with_git.ends_with(".git") {
                return repo_with_git[..repo_with_git.len() - 4].to_string();
            }
            return repo_with_git.to_string();
        }
        "unknown_repo".to_string()
    }

    fn build_plan(&self) -> ShiftPlan {
        let mut args = vec!["clone".to_string(), self.repo_url.clone()];
        if let Some(branch) = &self.branch {
            args.push("--branch".to_string());
            args.push(branch.clone());
        }
        if let Some(depth) = self.depth {
            args.push("--depth".to_string());
            args.push(depth.to_string());
        }

        args.push(self.target_dir.clone());
        let git_plan = Box::new(Cmd::new(
            "git".to_string(),
            args,
            Some(self.target_dir.clone()),
            None,
        ));

        ShiftPlan::new(
            format!("Clone GitHub repository {}", self.get_repo_name()),
            format!(
                "Clone GitHub repository {} into directory {}",
                self.get_repo_name(),
                self.target_dir
            ),
            vec![Box::new(CreateDir::new(self.target_dir.clone())), git_plan],
        )
    }
}

impl Shift for GitHubClone {
    fn apply(&self) -> ShiftResult<()> {
        let plan = self.build_plan();
        plan.apply()
    }

    fn revert(&self) -> ShiftResult<()> {
        let plan = self.build_plan();
        plan.revert()
    }

    fn is_applied(&self) -> ShiftResult<bool> {
        let path = Path::new(&self.target_dir);
        Ok(path.exists() && path.is_dir())
    }

    fn describe(&self) -> String {
        format!(
            "Clone GitHub repository {} into directory {}",
            self.get_repo_name(),
            self.target_dir
        )
    }
}

// Example usage
fn main() {
    // Create a shift plan for setting up a web project
    let web_project_plan = ShiftPlan::new(
        "Web Project Setup".to_string(),
        "Sets up a basic web project structure".to_string(),
        vec![
            Box::new(CreateDir::new("project".to_string())),
            Box::new(CreateDir::new("project/src".to_string())),
            Box::new(CreateDir::new("project/public".to_string())),
            Box::new(CreateFile::new(
                "project/public/index.html".to_string(),
                "<!DOCTYPE html><html><head><title>My Project</title></head><body><h1>Hello World</h1></body></html>".to_string(),
            )),
            Box::new(CreateFile::new(
                "project/src/main.js".to_string(),
                "console.log('Hello from main.js');".to_string(),
            )),
            Box::new(Cmd::new(
                "npm".to_string(),
                vec!["init".to_string(), "-y".to_string()],
                Some("project".to_string()),
                None,
            )),
        ],
    );

    if let Err(e) = web_project_plan.apply() {
        println!("Failed to apply shift plan: {:#?}", e);
    }

    let git_plan = GitHubClone::new(
        "git@github.com:GyrosOfWar/s3-proxy.git".into(),
        "s3-proxy".into(),
        Some("main".into()),
        None,
        None,
    );

    if let Err(e) = git_plan.apply() {
        println!("Failed to apply shift plan: {:#?}", e);
    }
}
