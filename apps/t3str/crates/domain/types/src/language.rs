//! Supported programming languages.

use serde::{Deserialize, Serialize};

/// Programming languages supported by t3str.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust (cargo test).
    Rust,
    /// Python (pytest).
    Python,
    /// Go (go test).
    Go,
    /// JavaScript/TypeScript (jest, mocha).
    #[serde(alias = "js", alias = "typescript", alias = "ts")]
    Javascript,
    /// PHP (phpunit, nette tester).
    Php,
    /// C# (.NET) (dotnet test).
    #[serde(alias = "c#", alias = "dotnet")]
    Csharp,
    /// Ruby (rspec, minitest).
    Ruby,
    /// Java (maven surefire, `JUnit`).
    Java,
    /// Elixir (mix test).
    Elixir,
}

impl Language {
    /// Returns the canonical lowercase string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Go => "go",
            Self::Javascript => "javascript",
            Self::Php => "php",
            Self::Csharp => "csharp",
            Self::Ruby => "ruby",
            Self::Java => "java",
            Self::Elixir => "elixir",
        }
    }
}

impl core::fmt::Display for Language {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
