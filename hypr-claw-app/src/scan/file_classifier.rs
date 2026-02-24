use std::fs;
use std::io::Read;
use std::path::Path;

/// File classification for scan results
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileClass {
    Config { subtype: ConfigType },
    Script { language: String },
    Source { language: String },
    Document,
    Media,
    Binary { reason: SkipReason },
    Data,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigType {
    Shell,
    Desktop,
    Editor,
    Git,
    Ssh,
    Environment,
    Toml,
    Yaml,
    Json,
    Ini,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    TooLarge(u64),
    BinaryExecutable,
    CompressedArchive,
}

/// Classify file by extension and content
pub fn classify_file(path: &Path, max_size: u64) -> Result<FileClass, std::io::Error> {
    let metadata = fs::metadata(path)?;

    // Skip large files
    if metadata.len() > max_size {
        return Ok(FileClass::Binary {
            reason: SkipReason::TooLarge(metadata.len()),
        });
    }

    // Check if binary executable
    if is_binary_executable(path)? {
        return Ok(FileClass::Binary {
            reason: SkipReason::BinaryExecutable,
        });
    }

    // Extension-based classification
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        return Ok(match ext_lower.as_str() {
            // Scripts
            "sh" | "bash" | "zsh" | "fish" => FileClass::Script {
                language: "shell".into(),
            },
            "py" => FileClass::Script {
                language: "python".into(),
            },
            "rb" => FileClass::Script {
                language: "ruby".into(),
            },
            "pl" => FileClass::Script {
                language: "perl".into(),
            },
            "lua" => FileClass::Script {
                language: "lua".into(),
            },

            // Source code
            "rs" => FileClass::Source {
                language: "rust".into(),
            },
            "c" | "h" => FileClass::Source {
                language: "c".into(),
            },
            "cpp" | "cc" | "cxx" | "hpp" => FileClass::Source {
                language: "cpp".into(),
            },
            "go" => FileClass::Source {
                language: "go".into(),
            },
            "java" => FileClass::Source {
                language: "java".into(),
            },
            "js" | "mjs" => FileClass::Source {
                language: "javascript".into(),
            },
            "ts" => FileClass::Source {
                language: "typescript".into(),
            },

            // Config files
            "toml" => FileClass::Config {
                subtype: ConfigType::Toml,
            },
            "yaml" | "yml" => FileClass::Config {
                subtype: ConfigType::Yaml,
            },
            "json" => FileClass::Config {
                subtype: ConfigType::Json,
            },
            "ini" | "cfg" | "conf" | "config" => FileClass::Config {
                subtype: ConfigType::Ini,
            },
            "env" => FileClass::Config {
                subtype: ConfigType::Environment,
            },

            // Documents
            "txt" | "md" | "rst" | "adoc" => FileClass::Document,
            "pdf" | "doc" | "docx" | "odt" | "rtf" => FileClass::Document,

            // Media
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "ico" => FileClass::Media,
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" => FileClass::Media,
            "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "wma" => FileClass::Media,

            // Archives
            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => FileClass::Binary {
                reason: SkipReason::CompressedArchive,
            },

            // Data
            "db" | "sqlite" | "sqlite3" => FileClass::Data,
            "log" => FileClass::Data,

            _ => FileClass::Other,
        });
    }

    // Check for dotfiles (configs without extension)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') {
            return Ok(classify_dotfile(name));
        }
    }

    Ok(FileClass::Other)
}

fn classify_dotfile(name: &str) -> FileClass {
    match name {
        ".bashrc" | ".bash_profile" | ".bash_logout" | ".zshrc" | ".zshenv" | ".profile" => {
            FileClass::Config {
                subtype: ConfigType::Shell,
            }
        }
        ".gitconfig" | ".gitignore" | ".gitattributes" => FileClass::Config {
            subtype: ConfigType::Git,
        },
        ".vimrc" | ".nvimrc" => FileClass::Config {
            subtype: ConfigType::Editor,
        },
        ".env" | ".envrc" => FileClass::Config {
            subtype: ConfigType::Environment,
        },
        _ => FileClass::Other,
    }
}

fn is_binary_executable(path: &Path) -> Result<bool, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 4];

    // Try to read first 4 bytes
    if file.read(&mut magic).is_err() {
        return Ok(false);
    }

    // Check magic numbers
    Ok(matches!(
        magic,
        // ELF: 0x7F 'E' 'L' 'F'
        [0x7F, b'E', b'L', b'F']
        // PE: 'M' 'Z'
        | [b'M', b'Z', _, _]
        // Mach-O: 0xFE 0xED 0xFA 0xCE or 0xCE 0xFA 0xED 0xFE
        | [0xFE, 0xED, 0xFA, 0xCE]
        | [0xCE, 0xFA, 0xED, 0xFE]
        // Mach-O 64-bit: 0xFE 0xED 0xFA 0xCF or 0xCF 0xFA 0xED 0xFE
        | [0xFE, 0xED, 0xFA, 0xCF]
        | [0xCF, 0xFA, 0xED, 0xFE]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_classify_rust_file() {
        let temp = std::env::temp_dir().join("test.rs");
        fs::write(&temp, "fn main() {}").unwrap();

        let class = classify_file(&temp, 100 * 1024 * 1024).unwrap();
        assert_eq!(
            class,
            FileClass::Source {
                language: "rust".into()
            }
        );

        fs::remove_file(temp).ok();
    }

    #[test]
    fn test_classify_config_file() {
        let temp = std::env::temp_dir().join("config.toml");
        fs::write(&temp, "[section]\nkey = \"value\"").unwrap();

        let class = classify_file(&temp, 100 * 1024 * 1024).unwrap();
        assert_eq!(
            class,
            FileClass::Config {
                subtype: ConfigType::Toml
            }
        );

        fs::remove_file(temp).ok();
    }

    #[test]
    fn test_classify_large_file() {
        let temp = std::env::temp_dir().join("large.bin");
        let large_data = vec![0u8; 200 * 1024 * 1024]; // 200 MB
        fs::write(&temp, large_data).unwrap();

        let class = classify_file(&temp, 100 * 1024 * 1024).unwrap();
        match class {
            FileClass::Binary {
                reason: SkipReason::TooLarge(size),
            } => {
                assert!(size > 100 * 1024 * 1024);
            }
            _ => panic!("Expected Binary with TooLarge"),
        }

        fs::remove_file(temp).ok();
    }

    #[test]
    fn test_classify_dotfile() {
        let temp = std::env::temp_dir().join(".bashrc");
        fs::write(&temp, "alias ll='ls -la'").unwrap();

        let class = classify_file(&temp, 100 * 1024 * 1024).unwrap();
        assert_eq!(
            class,
            FileClass::Config {
                subtype: ConfigType::Shell
            }
        );

        fs::remove_file(temp).ok();
    }

    #[test]
    fn test_is_binary_executable() {
        // Create a fake ELF file
        let temp = std::env::temp_dir().join("test_elf");
        let mut file = fs::File::create(&temp).unwrap();
        file.write_all(&[0x7F, b'E', b'L', b'F']).unwrap();

        assert!(is_binary_executable(&temp).unwrap());

        fs::remove_file(temp).ok();
    }

    #[test]
    fn test_classify_script() {
        let temp = std::env::temp_dir().join("script.py");
        fs::write(&temp, "#!/usr/bin/env python3\nprint('hello')").unwrap();

        let class = classify_file(&temp, 100 * 1024 * 1024).unwrap();
        assert_eq!(
            class,
            FileClass::Script {
                language: "python".into()
            }
        );

        fs::remove_file(temp).ok();
    }
}
