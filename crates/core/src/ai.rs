// ai.rs — Shared AI readiness checks (desktop parity: TokenMix only).

pub fn ai_ready(provider: &str, api_key: &str, model: &str) -> Result<(), String> {
    if provider != "tokenmix" {
        return Err("Unsupported AI provider. TokenMix is required.".to_string());
    }
    if api_key.trim().is_empty() {
        return Err(
            "No TokenMix API key set. Configure it in Settings → AI Models or Admin → Platform credentials."
                .to_string(),
        );
    }
    if model.trim().is_empty() {
        return Err("No model set. Go to Settings → AI Models.".to_string());
    }
    Ok(())
}

/// Resolve a per-function model slot, falling back to the default model.
pub fn resolve_slot_model(slot_model: &str, default_model: &str) -> Result<String, String> {
    let m = if slot_model.trim().is_empty() {
        default_model
    } else {
        slot_model
    };
    if m.trim().is_empty() {
        return Err("No model set. Go to Settings → AI Models.".to_string());
    }
    Ok(m.to_string())
}
