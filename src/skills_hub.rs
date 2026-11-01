// src/skills_hub.rs
// Hub Unificado de Skills: conecta y sincroniza todas las Skills de OpenClaw y de Antigravity (Gemini 4 Argon)
// para que Claudia Desktop pueda descubrirlas, inyectar sus SKILL.md (Progressive Disclosure) y ejecutarlas.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub origin: String, // "openclaw", "antigravity", "openclaw+antigravity"
    pub skill_md_path: PathBuf,
    pub base_dir: PathBuf,
}

/// Sincroniza mediante symlinks las skills entre OpenClaw (~/.openclaw/skills)
/// y Antigravity (~/.gemini/config/skills) para que ambos motores compartan el 100% del catálogo.
pub fn sync_openclaw_and_antigravity_skills() {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/jejej"));
    let openclaw_skills_dir = home.join(".openclaw/skills");
    let agy_skills_dir = home.join(".gemini/config/skills");

    let _ = std::fs::create_dir_all(&openclaw_skills_dir);
    let _ = std::fs::create_dir_all(&agy_skills_dir);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        // 1. Vincular skills nativas y plugins de Antigravity hacia OpenClaw y ~/.gemini/config/skills
        let agy_builtin = home.join(".gemini/antigravity/builtin/skills");
        if let Ok(entries) = std::fs::read_dir(&agy_builtin) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("SKILL.md").exists() {
                    if let Some(name) = path.file_name() {
                        let dst_oc = openclaw_skills_dir.join(name);
                        let dst_agy = agy_skills_dir.join(name);
                        if !dst_oc.exists() {
                            let _ = symlink(&path, &dst_oc);
                        }
                        if !dst_agy.exists() {
                            let _ = symlink(&path, &dst_agy);
                        }
                    }
                }
            }
        }

        let android_plugin_base = home.join(".gemini/config/plugins/android-cli-plugin/skills");
        if android_plugin_base.join("SKILL.md").exists() {
            for target_dir in [&openclaw_skills_dir, &agy_skills_dir] {
                let dst = target_dir.join("android-cli");
                if !dst.exists() {
                    let _ = symlink(&android_plugin_base, &dst);
                }
            }
        }
        for sub in ["device-hub", "compose-preview"] {
            let sub_dir = android_plugin_base.join(sub);
            if sub_dir.join("SKILL.md").exists() {
                let link_name = format!("android-{}", sub);
                for target_dir in [&openclaw_skills_dir, &agy_skills_dir] {
                    let dst = target_dir.join(&link_name);
                    if !dst.exists() {
                        let _ = symlink(&sub_dir, &dst);
                    }
                }
            }
        }

        // 2. Vincular skills de OpenClaw (~/.agents/skills, ~/.openclaw/workspace/skills, ~/.openclaw/skills) hacia Antigravity
        for source_root in [
            home.join(".agents/skills"),
            home.join(".openclaw/workspace/skills"),
            openclaw_skills_dir.clone(),
        ] {
            if let Ok(entries) = std::fs::read_dir(&source_root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let resolved = std::fs::canonicalize(&path).unwrap_or(path.clone());
                    if resolved.is_dir() && resolved.join("SKILL.md").exists() {
                        if let Some(name) = path.file_name() {
                            let dst_agy = agy_skills_dir.join(name);
                            if !dst_agy.exists() {
                                let _ = symlink(&resolved, &dst_agy);
                            }
                            let dst_oc = openclaw_skills_dir.join(name);
                            if !dst_oc.exists() {
                                let _ = symlink(&resolved, &dst_oc);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Parsea el frontmatter YAML (`name` y `description`) de un archivo `SKILL.md`
fn parse_skill_md(skill_md_path: &Path, default_name: &str, origin: &str) -> Option<SkillEntry> {
    let content = std::fs::read_to_string(skill_md_path).ok()?;
    let base_dir = skill_md_path.parent()?.to_path_buf();

    let mut name = default_name.to_string();
    let mut description = String::new();

    if content.starts_with("---") {
        if let Some(end_idx) = content[3..].find("\n---") {
            let frontmatter = &content[3..3 + end_idx];
            let mut in_multiline_desc = false;

            for line in frontmatter.lines() {
                let trimmed = line.trim();
                if in_multiline_desc {
                    if line.starts_with("  ") || line.starts_with('\t') {
                        if !description.is_empty() {
                            description.push(' ');
                        }
                        description.push_str(trimmed);
                        continue;
                    } else {
                        in_multiline_desc = false;
                    }
                }

                if let Some(rest) = trimmed.strip_prefix("name:") {
                    let val = rest.trim().trim_matches('"').trim_matches('\'');
                    if !val.is_empty() {
                        name = val.to_string();
                    }
                } else if let Some(rest) = trimmed.strip_prefix("description:") {
                    let val = rest.trim();
                    if val == ">-" || val == ">" || val == "|" {
                        in_multiline_desc = true;
                    } else {
                        description = val.trim_matches('"').trim_matches('\'').to_string();
                    }
                }
            }
        }
    }

    if description.is_empty() {
        // Tomar la primera línea de texto útil fuera del frontmatter
        for line in content.lines().skip(1) {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with("---") && !t.starts_with('#') && !t.starts_with('{') {
                description = t.chars().take(180).collect();
                break;
            }
        }
    }

    Some(SkillEntry {
        name,
        description,
        origin: origin.to_string(),
        skill_md_path: skill_md_path.to_path_buf(),
        base_dir,
    })
}

/// Descubre todas las skills instaladas tanto en OpenClaw como en Antigravity
pub fn discover_all_skills() -> Vec<SkillEntry> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/jejej"));
    let mut map: HashMap<String, SkillEntry> = HashMap::new();

    let scan_sources: Vec<(PathBuf, &str)> = vec![
        (home.join(".gemini/antigravity/builtin/skills"), "antigravity"),
        (
            home.join(".gemini/config/plugins/android-cli-plugin/skills"),
            "antigravity",
        ),
        (home.join(".agents/skills"), "openclaw"),
        (home.join(".openclaw/workspace/skills"), "openclaw"),
        (
            home.join(".npm-global/lib/node_modules/openclaw/skills"),
            "openclaw",
        ),
        (home.join(".openclaw/skills"), "openclaw+antigravity"),
        (home.join(".gemini/config/skills"), "openclaw+antigravity"),
    ];

    for (root_dir, origin) in scan_sources {
        if !root_dir.exists() {
            continue;
        }
        // Caso especial si el propio directorio tiene SKILL.md (como android-cli-plugin/skills/SKILL.md)
        let direct_md = root_dir.join("SKILL.md");
        if direct_md.exists() {
            if let Some(entry) = parse_skill_md(&direct_md, "android-cli", origin) {
                map.entry(entry.name.clone()).or_insert(entry);
            }
        }

        if let Ok(entries) = std::fs::read_dir(&root_dir) {
            for e in entries.flatten() {
                let path = e.path();
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    let folder_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("skill");
                    if let Some(mut entry) = parse_skill_md(&skill_md, folder_name, origin) {
                        // Clasificar origen real según la ruta canónica
                        if let Ok(canon) = std::fs::canonicalize(&skill_md) {
                            let canon_str = canon.to_string_lossy();
                            if canon_str.contains(".gemini/antigravity")
                                || canon_str.contains("android-cli-plugin")
                            {
                                entry.origin = "antigravity".to_string();
                            } else if canon_str.contains(".openclaw")
                                || canon_str.contains(".agents/skills")
                                || canon_str.contains("node_modules/openclaw")
                            {
                                entry.origin = "openclaw".to_string();
                            }
                        }
                        map.entry(entry.name.clone()).or_insert(entry);
                    }
                }
            }
        }
    }

    let mut list: Vec<SkillEntry> = map.into_values().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Lee el contenido completo de un `SKILL.md` (truncado de forma segura para no saturar el contexto)
pub fn read_skill_instructions(skill: &SkillEntry) -> String {
    if let Ok(content) = std::fs::read_to_string(&skill.skill_md_path) {
        let trimmed: String = content.chars().take(3800).collect();
        return format!(
            "\n=== SKILL ACTIVA: {} (Origen: {}, BaseDir: {}) ===\n{}\n=== FIN SKILL {} ===\n",
            skill.name,
            skill.origin,
            skill.base_dir.display(),
            trimmed,
            skill.name
        );
    }
    String::new()
}

/// Detecta qué skills de OpenClaw o Antigravity son relevantes para el mensaje del usuario
/// (Progressive Disclosure automático + soporte explícito para `/skill <nombre>` o `"usa la skill..."`).
pub fn match_skills_for_query(user_msg: &str) -> Vec<SkillEntry> {
    let lower = user_msg.to_lowercase();
    let all_skills = discover_all_skills();
    let mut matched: Vec<SkillEntry> = Vec::new();

    // 1. Coincidencia explícita por nombre de skill en el mensaje
    for skill in &all_skills {
        let sname = skill.name.to_lowercase();
        if sname.len() >= 3 && lower.contains(&sname) {
            matched.push(skill.clone());
        }
    }

    // 2. Coincidencia semántica / por intención (español + inglés)
    let keyword_map: &[(&[&str], &[&str])] = &[
        (
            &["pdf", "extraer texto", "leer pdf", "crear pdf"],
            &["pdf", "nano-pdf"],
        ),
        (
            &["word", "docx", "documento word", "informe en word"],
            &["docx"],
        ),
        (
            &["excel", "xlsx", "planilla", "csv", "hoja de calculo", "hoja de cálculo"],
            &["xlsx"],
        ),
        (
            &[
                "automatiza el navegador",
                "scraping",
                "rellena el formulario",
                "agent-browser",
                "testear web",
                "navegador automatico",
            ],
            &["agent-browser", "unbrowse"],
        ),
        (
            &["clima", "temperatura", "lluvia", "pronostico", "pronóstico", "weather"],
            &["weather"],
        ),
        (
            &[
                "diagrama",
                "excalidraw",
                "mapa conceptual",
                "arquitectura svg",
                "flowchart",
                "widget html",
            ],
            &["diagram-maker", "generative_ui"],
        ),
        (
            &[
                "android",
                "adb",
                "avd",
                "emulador",
                "apk",
                "jetpack compose",
                "compose preview",
            ],
            &["android-cli", "android-device-hub", "compose-preview"],
        ),
        (
            &["voz japonesa", "piper", "tts", "sintetizar voz", "hablar en japones"],
            &["piper-tts-japanese-voice-setup"],
        ),
        (&["vercel", "deploy", "desplegar web"], &["deploy-to-vercel"]),
        (
            &["react", "next.js", "nextjs", "optimizar react"],
            &["vercel-react-best-practices"],
        ),
        (
            &["nmap", "escanear red", "puertos abiertos", "reconocimiento red"],
            &["nmap-recon-1.0.0"],
        ),
        (
            &["cron", "automatizacion", "automatización", "cada mañana", "tarea programada"],
            &["automation", "taskflow"],
        ),
        (
            &["plugin", "clawhub", "buscar skill", "instalar skill", "crear skill"],
            &["plugin", "clawhub", "find-skills", "skill-creator", "agy-customizations"],
        ),
        (
            &["antigravity", "agy", "atajos agy", "comandos agy"],
            &["antigravity-guide", "agy-customizations"],
        ),
        (
            &["taskflow", "multi-agente", "arquitecto y coders"],
            &["taskflow-coding", "taskflow"],
        ),
    ];

    for (keywords, target_skills) in keyword_map {
        if keywords.iter().any(|k| lower.contains(k)) {
            for target in *target_skills {
                if !matched.iter().any(|m| m.name.eq_ignore_ascii_case(target)) {
                    if let Some(found) = all_skills
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(target))
                    {
                        matched.push(found.clone());
                    }
                }
            }
        }
    }

    // Limitar a máximo 2 skills inyectadas por turno para mantener el prompt ágil
    matched.truncate(2);
    matched
}

/// Genera un resumen compacto del catálogo unificado de skills para el prompt del sistema
pub fn build_skills_catalog_summary() -> String {
    let all = discover_all_skills();
    let oc_count = all.iter().filter(|s| s.origin == "openclaw").count();
    let agy_count = all.iter().filter(|s| s.origin == "antigravity").count();

    let names: Vec<String> = all
        .iter()
        .map(|s| {
            let tag = if s.origin == "antigravity" { "AGY" } else { "OC" };
            format!("{}[{}]", s.name, tag)
        })
        .collect();

    format!(
        "Catálogo Unificado de Skills activas ({} total: {} OpenClaw + {} Antigravity): {}",
        all.len(),
        oc_count,
        agy_count,
        names.join(", ")
    )
}

/// Respuesta rápida cuando el usuario pregunta qué skills tiene Claudia o usa `/skills`
pub fn format_skills_overview_for_bubble() -> String {
    let all = discover_all_skills();
    let agy: Vec<String> = all
        .iter()
        .filter(|s| s.origin == "antigravity")
        .map(|s| s.name.clone())
        .collect();
    let oc: Vec<String> = all
        .iter()
        .filter(|s| s.origin != "antigravity")
        .take(14)
        .map(|s| s.name.clone())
        .collect();

    format!(
        "Tengo {} skills sincronizadas choom! De Antigravity ({}): {}. Y de OpenClaw: {}, entre otras. Pídeme usar cualquiera con '/skill nombre' o en lenguaje natural.",
        all.len(),
        agy.len(),
        agy.join(", "),
        oc.join(", ")
    )
}

/// Ejecución rápida de la skill `weather` usando `wttr.in` si el usuario pregunta por el clima
pub async fn quick_weather_skill(location_hint: &str) -> Result<String, String> {
    let loc = if location_hint.trim().is_empty() {
        "Santiago"
    } else {
        location_hint.trim()
    };
    let url = format!("https://wttr.in/{}?format=%l:+%c+%t+(sensación+%f),+humedad+%h,+viento+%w", loc.replace(' ', "+"));
    let output = tokio::time::timeout(
        tokio::time::Duration::from_secs(6),
        tokio::process::Command::new("curl")
            .arg("-s")
            .arg("-A")
            .arg("curl")
            .arg(&url)
            .output(),
    )
    .await
    .map_err(|_| "Timeout consultando wttr.in".to_string())?
    .map_err(|e| e.to_string())?;

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !text.is_empty() && !text.contains("Unknown location") && !text.starts_with('<') {
        Ok(format!("Skill [weather] en vivo: {}.", text))
    } else {
        Err("No se pudo obtener el clima rápido".to_string())
    }
}
