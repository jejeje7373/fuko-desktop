// src/connections_config.rs
// Configuración personalizada de conexiones para OpenClaw y Antigravity.
// Permite al usuario configurar manualmente sus endpoints, modelos, tokens y argumentos,
// almacenándolos en ~/.config/fuko/connections.json.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionsConfig {
    // OpenClaw
    pub openclaw_mode: String,        // "cli" (agent exec / infer) o "gateway" (HTTP)
    pub openclaw_endpoint: String,    // "http://127.0.0.1:18789" o ruta de binario
    pub openclaw_token: String,       // Token auth opcional
    pub openclaw_model: String,       // Modelo de OpenClaw (ej. "nvidia/nemotron-3-ultra-550b-a55b")
    pub openclaw_custom_args: String, // Argumentos extra

    // Antigravity
    pub antigravity_bin: String,       // "agy" o ruta absoluta del CLI
    pub antigravity_model: String,     // Modelo configurado por el usuario (o vacío para default de agy)
    pub antigravity_args: String,      // Ej. "--dangerously-skip-permissions"
    pub antigravity_timeout_secs: u64, // Timeout en segundos (por defecto 45)
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        let agy_default = if Path::new("/home/jejej/.gemini/bin/agy").exists() {
            "/home/jejej/.gemini/bin/agy".to_string()
        } else if Path::new("/usr/local/bin/agy").exists() {
            "/usr/local/bin/agy".to_string()
        } else {
            "agy".to_string()
        };

        Self {
            openclaw_mode: "cli".to_string(),
            openclaw_endpoint: "http://127.0.0.1:18789".to_string(),
            openclaw_token: "".to_string(),
            openclaw_model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
            openclaw_custom_args: "".to_string(),

            antigravity_bin: agy_default,
            antigravity_model: "gemini-3.8-flash-high".to_string(), // Modelo rápido con cuota activa en Antigravity
            antigravity_args: "--dangerously-skip-permissions --disable-slash-commands".to_string(),
            antigravity_timeout_secs: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub message: String,
    pub details: String,
}

fn get_config_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/jejej".to_string());
    PathBuf::from(home).join(".config/fuko/connections.json")
}

pub fn load_connections_config() -> ConnectionsConfig {
    let path = get_config_file_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<ConnectionsConfig>(&content) {
                return cfg;
            }
        }
    }
    ConnectionsConfig::default()
}

pub fn save_connections_config(config: &ConnectionsConfig) -> Result<(), String> {
    let path = get_config_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Error serializando config: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Error guardando config: {}", e))?;
    Ok(())
}

// =========================================================================
// COMANDOS TAURI
// =========================================================================

#[tauri::command]
pub async fn get_connections_config_cmd() -> Result<ConnectionsConfig, String> {
    Ok(load_connections_config())
}

#[tauri::command]
pub async fn save_connections_config_cmd(config: ConnectionsConfig) -> Result<String, String> {
    save_connections_config(&config)?;
    println!(
        "[Fuko] Conexiones guardadas: OpenClaw model={}, Antigravity bin={}, model={}",
        config.openclaw_model, config.antigravity_bin, config.antigravity_model
    );
    Ok("Configuración de conexiones guardada exitosamente.".to_string())
}

#[tauri::command]
pub async fn test_openclaw_connection_cmd(
    config: Option<ConnectionsConfig>,
) -> Result<ConnectionTestResult, String> {
    let cfg = config.unwrap_or_else(load_connections_config);
    let start = Instant::now();

    if cfg.openclaw_mode == "gateway" && cfg.openclaw_endpoint.starts_with("http") {
        // Test HTTP Gateway
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;

        let url = format!("{}/health", cfg.openclaw_endpoint.trim_end_matches('/'));
        let mut req = client.get(&url);
        if !cfg.openclaw_token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", cfg.openclaw_token));
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let latency = start.elapsed().as_millis() as u64;
                if status.is_success() {
                    Ok(ConnectionTestResult {
                        ok: true,
                        latency_ms: latency,
                        message: format!("Gateway OpenClaw ONLINE ({})", status),
                        details: format!("Conectado a {}", cfg.openclaw_endpoint),
                    })
                } else {
                    Ok(ConnectionTestResult {
                        ok: false,
                        latency_ms: latency,
                        message: format!("Respuesta no exitosa: {}", status),
                        details: format!("URL: {}", url),
                    })
                }
            }
            Err(e) => {
                let latency = start.elapsed().as_millis() as u64;
                Ok(ConnectionTestResult {
                    ok: false,
                    latency_ms: latency,
                    message: format!("Error conectando a Gateway OpenClaw: {}", e),
                    details: format!("Verifica que el Gateway esté corriendo en {}", cfg.openclaw_endpoint),
                })
            }
        }
    } else {
        // Test OpenClaw CLI
        let bin = if Path::new(&cfg.openclaw_endpoint).exists() {
            cfg.openclaw_endpoint.as_str()
        } else if Path::new("/home/jejej/.nvm/versions/node/v22.12.0/bin/openclaw").exists() {
            "/home/jejej/.nvm/versions/node/v22.12.0/bin/openclaw"
        } else {
            "openclaw"
        };

        let mut cmd = Command::new(bin);
        cmd.args(["--version"]);

        match tokio::time::timeout(tokio::time::Duration::from_secs(5), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => {
                let latency = start.elapsed().as_millis() as u64;
                let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
                Ok(ConnectionTestResult {
                    ok: true,
                    latency_ms: latency,
                    message: format!("OpenClaw CLI OK ({})", ver),
                    details: format!("Binario: {} | Modelo: {}", bin, cfg.openclaw_model),
                })
            }
            Ok(Ok(output)) => {
                let latency = start.elapsed().as_millis() as u64;
                let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Ok(ConnectionTestResult {
                    ok: false,
                    latency_ms: latency,
                    message: "OpenClaw CLI devolvió error".to_string(),
                    details: err,
                })
            }
            Ok(Err(e)) => {
                let latency = start.elapsed().as_millis() as u64;
                Ok(ConnectionTestResult {
                    ok: false,
                    latency_ms: latency,
                    message: format!("No se pudo ejecutar OpenClaw CLI: {}", e),
                    details: format!("Ruta probada: {}", bin),
                })
            }
            Err(_) => {
                Ok(ConnectionTestResult {
                    ok: false,
                    latency_ms: 5000,
                    message: "Timeout ejecutando OpenClaw CLI".to_string(),
                    details: "El proceso tardó más de 5 segundos".to_string(),
                })
            }
        }
    }
}

#[tauri::command]
pub async fn test_antigravity_connection_cmd(
    config: Option<ConnectionsConfig>,
) -> Result<ConnectionTestResult, String> {
    let cfg = config.unwrap_or_else(load_connections_config);
    let start = Instant::now();

    let bin = if Path::new(&cfg.antigravity_bin).exists() {
        cfg.antigravity_bin.as_str()
    } else if Path::new("/home/jejej/.gemini/bin/agy").exists() {
        "/home/jejej/.gemini/bin/agy"
    } else {
        "agy"
    };

    let mut cmd = Command::new(bin);
    cmd.args(["--version"]);

    match tokio::time::timeout(tokio::time::Duration::from_secs(8), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => {
            let latency = start.elapsed().as_millis() as u64;
            let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let model_desc = if cfg.antigravity_model.is_empty() {
                "(Modelo por defecto de Antigravity)".to_string()
            } else {
                format!("Modelo: {}", cfg.antigravity_model)
            };
            Ok(ConnectionTestResult {
                ok: true,
                latency_ms: latency,
                message: format!("Antigravity CLI ONLINE ({})", if ver.is_empty() { "agy OK" } else { &ver }),
                details: format!("Ejecutable: {} | {}", bin, model_desc),
            })
        }
        Ok(Ok(output)) => {
            let latency = start.elapsed().as_millis() as u64;
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Ok(ConnectionTestResult {
                ok: false,
                latency_ms: latency,
                message: "Antigravity CLI devolvió error".to_string(),
                details: err,
            })
        }
        Ok(Err(e)) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(ConnectionTestResult {
                ok: false,
                latency_ms: latency,
                message: format!("No se pudo ejecutar Antigravity CLI: {}", e),
                details: format!("Ruta probada: {}", bin),
            })
        }
        Err(_) => {
            Ok(ConnectionTestResult {
                ok: false,
                latency_ms: 8000,
                message: "Timeout consultando Antigravity CLI".to_string(),
                details: "El proceso agy no respondió en 8 segundos".to_string(),
            })
        }
    }
}
