use machineid_rs::{Encryption, HWIDComponent, IdBuilder};

const PARAPHRASE: &str = "forge_key";

/// Gets or creates a persistent client ID for non-Android platforms
pub fn get_or_create_client_id() -> anyhow::Result<String> {
    let mut builder = IdBuilder::new(Encryption::SHA256);
    builder
        .add_component(HWIDComponent::SystemID)
        .add_component(HWIDComponent::CPUCores);

    builder
        .build(PARAPHRASE)
        .map_err(|e| anyhow::anyhow!("Failed to generate machine ID: {e}"))
}
