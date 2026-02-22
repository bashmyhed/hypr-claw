use crate::config::{Config, LLMProvider};
use anyhow::{Result, Context};
use std::io::{self, Write};

const NVIDIA_API_KEY_NAME: &str = "llm/nvidia_api_key";

pub fn run_bootstrap() -> Result<Config> {
    println!("\nNo LLM provider configured.");
    println!("Select provider:");
    println!("1. NVIDIA Kimi (cloud)");
    println!("2. Local model");
    print!("\nChoice [1-2]: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    match choice {
        "1" => bootstrap_nvidia(),
        "2" => bootstrap_local(),
        _ => {
            anyhow::bail!("Invalid choice. Please select 1 or 2.");
        }
    }
}

fn bootstrap_nvidia() -> Result<Config> {
    println!("\nEnter NVIDIA API key:");
    let api_key = rpassword::read_password()
        .context("Failed to read API key")?;

    if api_key.trim().is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    // Store encrypted credential
    let master_key = get_or_create_master_key()?;
    let cred_store = hypr_claw::infra::credential_store::CredentialStore::new(
        "./data/credentials",
        &master_key,
    )?;
    
    cred_store.store_secret(NVIDIA_API_KEY_NAME, api_key.trim())?;

    let config = Config {
        provider: LLMProvider::Nvidia,
        model: "moonshotai/kimi-k2.5".to_string(),
    };

    config.save()?;
    println!("✅ NVIDIA provider configured");

    Ok(config)
}

fn bootstrap_local() -> Result<Config> {
    print!("\nEnter local LLM base URL: ");
    io::stdout().flush()?;
    
    let mut base_url = String::new();
    io::stdin().read_line(&mut base_url)?;
    let base_url = base_url.trim().to_string();

    if base_url.is_empty() {
        anyhow::bail!("Base URL cannot be empty");
    }

    let config = Config {
        provider: LLMProvider::Local { base_url },
        model: "default".to_string(),
    };

    config.save()?;
    println!("✅ Local provider configured");

    Ok(config)
}

pub fn get_nvidia_api_key() -> Result<String> {
    let master_key = get_or_create_master_key()?;
    let cred_store = hypr_claw::infra::credential_store::CredentialStore::new(
        "./data/credentials",
        &master_key,
    )?;
    
    cred_store.get_secret(NVIDIA_API_KEY_NAME)
        .context("NVIDIA API key not found. Run bootstrap again.")
}

pub fn delete_nvidia_api_key() -> Result<()> {
    let master_key = get_or_create_master_key()?;
    let cred_store = hypr_claw::infra::credential_store::CredentialStore::new(
        "./data/credentials",
        &master_key,
    )?;
    
    cred_store.delete_secret(NVIDIA_API_KEY_NAME)?;
    Ok(())
}

fn get_or_create_master_key() -> Result<[u8; 32]> {
    let key_path = "./data/.master_key";
    
    if std::path::Path::new(key_path).exists() {
        let key_bytes = std::fs::read(key_path)?;
        if key_bytes.len() != 32 {
            anyhow::bail!("Invalid master key length");
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        Ok(key)
    } else {
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        std::fs::write(key_path, key)?;
        Ok(key)
    }
}
